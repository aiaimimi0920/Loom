//! Executable containment and Windows legacy-path adaptation.

use std::path::{Path, PathBuf};

#[cfg(windows)]
pub(crate) fn process_path(path: &Path) -> PathBuf {
    use std::ffi::OsString;
    use std::iter;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    const LEGACY_MAX_DIRECTORY_PATH: usize = 248;
    if !path.is_absolute() || path.as_os_str().encode_wide().count() < LEGACY_MAX_DIRECTORY_PATH {
        return path.to_path_buf();
    }

    // CreateProcessW does not accept a verbatim (\\?\) current directory.
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let wide = canonical
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect::<Vec<_>>();
    let mut short = vec![0u16; 32_768];
    let written =
        unsafe { GetShortPathNameW(wide.as_ptr(), short.as_mut_ptr(), short.len() as u32) };
    if written == 0 || written as usize >= short.len() {
        return path.to_path_buf();
    }
    let short = &short[..written as usize];
    const VERBATIM_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const VERBATIM_UNC_PREFIX: &[u16] = &[
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];
    if let Some(rest) = short.strip_prefix(VERBATIM_UNC_PREFIX) {
        let ordinary_unc = [b'\\' as u16, b'\\' as u16]
            .into_iter()
            .chain(rest.iter().copied())
            .collect::<Vec<_>>();
        OsString::from_wide(&ordinary_unc).into()
    } else if let Some(rest) = short.strip_prefix(VERBATIM_PREFIX) {
        OsString::from_wide(rest).into()
    } else {
        OsString::from_wide(short).into()
    }
}

#[cfg(not(windows))]
pub(crate) fn process_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

pub fn executable_path_within(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let root = std::fs::canonicalize(root).map_err(|error| error.to_string())?;
    let candidate =
        std::fs::canonicalize(root.join(relative)).map_err(|error| error.to_string())?;
    if !candidate.starts_with(&root) || !candidate.is_file() {
        return Err("executable resolves outside its package root".to_owned());
    }
    Ok(candidate)
}
