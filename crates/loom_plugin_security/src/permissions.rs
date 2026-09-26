//! Platform-specific private ACL/mode repair for Loom control-plane files.

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(all(test, windows))]
#[test]
fn private_acl_accepts_forward_slash_windows_paths() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "loom-acl-separators-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    let file = root.join("private.txt");
    fs::write(&file, b"fixture").unwrap();
    let directory_result = restrict_private_path_permissions(
        Path::new(&root.to_string_lossy().replace('\\', "/")),
        true,
    );
    let file_result = restrict_private_path_permissions(
        Path::new(&file.to_string_lossy().replace('\\', "/")),
        false,
    );
    fs::remove_dir_all(&root).unwrap();
    directory_result.expect("forward-slash directory ACL");
    file_result.expect("forward-slash file ACL");
}

#[cfg(unix)]
pub fn restrict_private_path_permissions(path: &Path, directory: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(if directory { 0o700 } else { 0o600 });
    fs::set_permissions(path, permissions)
}

#[cfg(windows)]
pub fn restrict_private_path_permissions(path: &Path, directory: bool) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        SetFileSecurityW, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR,
    };

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    // Verbatim Win32 paths bypass slash normalization; preserve UTF-16 names.
    let wide = absolute
        .as_os_str()
        .encode_wide()
        .map(|unit| {
            if unit == b'/' as u16 {
                b'\\' as u16
            } else {
                unit
            }
        })
        .collect::<Vec<_>>();
    let mut extended = if wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16])
        || wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'.' as u16, b'\\' as u16])
    {
        wide
    } else if wide.starts_with(&[b'\\' as u16, b'\\' as u16]) {
        let mut value = r"\\?\UNC\".encode_utf16().collect::<Vec<_>>();
        value.extend_from_slice(&wide[2..]);
        value
    } else {
        let mut value = r"\\?\".encode_utf16().collect::<Vec<_>>();
        value.extend_from_slice(&wide);
        value
    };
    extended.push(0);
    let inheritance = if directory { "OICI" } else { "" };
    let current_user_sid = current_user_sid_string()?;
    let sddl = format!(
        "D:P(A;{inheritance};FA;;;{current_user_sid})(A;{inheritance};FA;;;OW)(A;{inheritance};FA;;;SY)\0"
    )
    .encode_utf16()
    .collect::<Vec<_>>();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let updated = unsafe {
        SetFileSecurityW(
            extended.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    };
    unsafe {
        LocalFree(descriptor.cast());
    }
    if updated == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub fn restrict_private_path_permissions(_path: &Path, _directory: bool) -> std::io::Result<()> {
    Ok(())
}

/// Repairs a symlink-free control-plane tree and reports permission-denied entries.
pub fn repair_private_tree_permissions(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    restrict_private_path_permissions(root, true).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!(
                "repair private directory permissions {}: {error}",
                root.display()
            ),
        )
    })?;
    let mut pending = vec![root.to_path_buf()];
    let mut quarantined = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "private control-plane tree contains a symbolic link: {}",
                        path.display()
                    ),
                ));
            }
            if let Err(error) = restrict_private_tree_entry_permissions(&path, file_type.is_dir()) {
                if error.kind() != std::io::ErrorKind::PermissionDenied {
                    return Err(std::io::Error::new(
                        error.kind(),
                        format!(
                            "repair private path permissions {}: {error}",
                            path.display()
                        ),
                    ));
                }
                let relative = path.strip_prefix(root).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "private ACL repair escaped its root",
                    )
                })?;
                quarantined.push(relative.to_path_buf());
                continue;
            }
            if file_type.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(quarantined)
}

fn restrict_private_tree_entry_permissions(path: &Path, directory: bool) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        // Installed runtimes need owner execute access after both install and startup repair.
        let mode = if directory {
            0o700
        } else {
            0o600 | (permissions.mode() & 0o100)
        };
        permissions.set_mode(mode);
        fs::set_permissions(path, permissions)
    }
    #[cfg(not(unix))]
    restrict_private_path_permissions(path, directory)
}

#[cfg(all(test, unix))]
#[test]
fn private_tree_repair_preserves_only_owner_execute_for_runtimes() {
    use std::os::unix::fs::PermissionsExt;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "loom-private-executable-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let executable = root.join("runtime");
    let data = root.join("settings.json");
    fs::write(&executable, b"#!/bin/sh\nexit 0\n").unwrap();
    fs::write(&data, b"{}").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o7777)).unwrap();
    fs::set_permissions(&data, fs::Permissions::from_mode(0o666)).unwrap();
    for _ in 0..2 {
        assert!(repair_private_tree_permissions(&root).unwrap().is_empty());
        assert_eq!(
            fs::metadata(&executable).unwrap().permissions().mode() & 0o7777,
            0o700
        );
        assert_eq!(
            fs::metadata(&data).unwrap().permissions().mode() & 0o7777,
            0o600
        );
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o7777,
            0o700
        );
        assert!(std::process::Command::new(&executable)
            .status()
            .unwrap()
            .success());
    }
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
fn current_user_sid_string() -> std::io::Result<String> {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let result = (|| -> std::io::Result<String> {
        let mut required = 0u32;
        unsafe {
            let _ = GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut required);
        }
        if required < size_of::<TOKEN_USER>() as u32 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Windows token user information is unavailable",
            ));
        }
        let mut buffer = vec![0u8; required as usize];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                required,
                &mut required,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let token_user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
        let mut sid_string = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(token_user.User.Sid, &mut sid_string) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut length = 0usize;
        unsafe {
            while *sid_string.add(length) != 0 {
                length += 1;
            }
        }
        let sid =
            String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(sid_string, length) });
        unsafe {
            LocalFree(sid_string.cast());
        }
        Ok(sid)
    })();
    unsafe {
        CloseHandle(token);
    }
    result
}
