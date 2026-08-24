use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use super::error::CredentialError;

#[cfg(windows)]
pub(super) fn protect_value(value: &[u8]) -> Result<(String, String), CredentialError> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: value.len() as u32,
        pbData: value.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let protected = unsafe {
        CryptProtectData(
            &mut input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if protected == 0 {
        return Err(CredentialError::Protection(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
    let encoded = BASE64.encode(bytes);
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok((encoded, "windows-dpapi-current-user".to_owned()))
}

#[cfg(windows)]
pub(super) fn unprotect_value(value: &str, protection: &str) -> Result<Vec<u8>, CredentialError> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    if protection != "windows-dpapi-current-user" {
        return Err(CredentialError::Protection(format!(
            "unsupported protection `{protection}`"
        )));
    }
    let mut encrypted = BASE64.decode(value.as_bytes())?;
    let mut input = CRYPT_INTEGER_BLOB {
        cbData: encrypted.len() as u32,
        pbData: encrypted.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let unprotected = unsafe {
        CryptUnprotectData(
            &mut input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if unprotected == 0 {
        return Err(CredentialError::Protection(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
    let result = bytes.to_vec();
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok(result)
}

#[cfg(not(windows))]
pub(super) fn protect_value(value: &[u8]) -> Result<(String, String), CredentialError> {
    Ok((BASE64.encode(value), "local-file-base64".to_owned()))
}

#[cfg(not(windows))]
pub(super) fn unprotect_value(value: &str, protection: &str) -> Result<Vec<u8>, CredentialError> {
    if protection != "local-file-base64" {
        return Err(CredentialError::Protection(format!(
            "unsupported protection `{protection}`"
        )));
    }
    Ok(BASE64.decode(value.as_bytes())?)
}
