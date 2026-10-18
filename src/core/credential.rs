use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

const ENTROPY: &[u8] = b"WhisperGUI.Credential.v1";
const DESCRIPTION: &str = "WhisperGUI";

pub fn protect(plaintext: &str) -> String {
    if plaintext.is_empty() {
        return String::new();
    }
    let input = blob(plaintext.as_bytes());
    let entropy = blob(ENTROPY);
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let description = wide(DESCRIPTION);
    let protected = unsafe {
        CryptProtectData(
            &input,
            description.as_ptr(),
            &entropy,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    let result = match protected {
        0 => String::new(),
        _ => STANDARD.encode(copy_and_free(output)),
    };
    free(input);
    free(entropy);
    result
}

pub fn unprotect(ciphertext: &str) -> String {
    if ciphertext.trim().is_empty() {
        return String::new();
    }
    let Ok(bytes) = STANDARD.decode(ciphertext) else {
        return String::new();
    };
    let input = blob(&bytes);
    let entropy = blob(ENTROPY);
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let unprotected = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            &entropy,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    let result = match unprotected {
        0 => String::new(),
        _ => String::from_utf8(copy_and_free(output)).unwrap_or_default(),
    };
    free(input);
    free(entropy);
    result
}

pub fn is_valid_cipher(value: &str) -> bool {
    STANDARD.decode(value).is_ok()
}

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    let mut owned = bytes.to_vec();
    let pointer = owned.as_mut_ptr();
    std::mem::forget(owned);
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: pointer,
    }
}

fn copy_and_free(source: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    if source.pbData.is_null() || source.cbData == 0 {
        return Vec::new();
    }
    let slice = unsafe { std::slice::from_raw_parts(source.pbData, source.cbData as usize) };
    let copy = slice.to_vec();
    unsafe {
        LocalFree(source.pbData as *mut core::ffi::c_void);
    }
    copy
}

fn free(value: CRYPT_INTEGER_BLOB) {
    if value.pbData.is_null() {
        return;
    }
    unsafe {
        std::ptr::write_bytes(value.pbData, 0, value.cbData as usize);
        let _ = Vec::from_raw_parts(value.pbData, value.cbData as usize, value.cbData as usize);
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
