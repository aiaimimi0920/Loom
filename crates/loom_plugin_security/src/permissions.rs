//! Platform-specific private ACL/mode repair for Loom control-plane files.

use std::fs;
use std::path::{Path, PathBuf};

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
    let wide = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
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
            if let Err(error) = restrict_private_path_permissions(&path, file_type.is_dir()) {
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
