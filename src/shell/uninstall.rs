use std::process::ExitCode;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
};

pub const PRODUCT_NAME: &str = "Whisper GUI";

pub const UNINSTALL_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstalledProduct {
    uninstall_string: String,
    quiet_uninstall_string: Option<String>,
    product_code: Option<String>,
}

pub fn run(args: Vec<String>) -> ExitCode {
    let check_only = has_flag(&args, &["--check"]);
    let quiet = has_flag(&args, &["/quiet", "--quiet"]);
    let products = find_products();
    if check_only {
        return if products.is_empty() {
            ExitCode::from(2)
        } else {
            ExitCode::SUCCESS
        };
    }
    if products.is_empty() {
        return ExitCode::from(2);
    }
    for product in &products {
        match run_uninstall(product, quiet) {
            Ok(code) if code == 0 || code == 3010 => {}
            Ok(code) => return ExitCode::from((code & 0xFF) as u8),
            Err(_) => return ExitCode::from(3),
        }
    }
    ExitCode::SUCCESS
}

fn has_flag(args: &[String], flags: &[&str]) -> bool {
    args.iter()
        .any(|arg| flags.iter().any(|flag| flag.eq_ignore_ascii_case(arg)))
}

fn find_products() -> Vec<InstalledProduct> {
    let mut products: Vec<InstalledProduct> = Vec::new();
    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let Some(key) = open_key(root, UNINSTALL_KEY) else {
            continue;
        };
        let mut index = 0;
        while let Some(sub_key_name) = enum_key(key, index) {
            index += 1;
            let path = format!("{UNINSTALL_KEY}\\{sub_key_name}");
            let Some(item) = open_key(root, &path) else {
                continue;
            };
            let display_name = read_string(item, "DisplayName").unwrap_or_default();
            let uninstall_string = read_string(item, "UninstallString").unwrap_or_default();
            let quiet_uninstall_string = read_string(item, "QuietUninstallString");
            unsafe {
                RegCloseKey(item);
            }
            if display_name != PRODUCT_NAME || uninstall_string.is_empty() {
                continue;
            }
            let product = InstalledProduct {
                product_code: extract_product_code(&uninstall_string),
                uninstall_string,
                quiet_uninstall_string,
            };
            if !products.contains(&product) {
                products.push(product);
            }
        }
        unsafe {
            RegCloseKey(key);
        }
    }
    products
}

fn open_key(root: HKEY, path: &str) -> Option<HKEY> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut handle: HKEY = std::ptr::null_mut();
    let status = unsafe { RegOpenKeyExW(root, wide.as_ptr(), 0, KEY_READ, &mut handle) };
    if status == ERROR_SUCCESS {
        Some(handle)
    } else {
        None
    }
}

fn enum_key(key: HKEY, index: u32) -> Option<String> {
    let mut buffer = [0u16; 512];
    let mut length = buffer.len() as u32;
    let status = unsafe {
        RegEnumKeyExW(
            key,
            index,
            buffer.as_mut_ptr(),
            &mut length,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..length as usize]))
}

fn read_string(key: HKEY, name: &str) -> Option<String> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut kind: REG_VALUE_TYPE = REG_SZ;
    let mut size: u32 = 0;
    let status = unsafe {
        RegQueryValueExW(
            key,
            wide.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS || size == 0 {
        return None;
    }
    if kind != REG_SZ && kind != REG_EXPAND_SZ {
        return None;
    }
    let mut buffer = vec![0u8; size as usize];
    let status = unsafe {
        RegQueryValueExW(
            key,
            wide.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            buffer.as_mut_ptr(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let units: Vec<u16> = buffer
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    let text = String::from_utf16_lossy(&units);
    let trimmed = text.trim_end_matches('\0').to_string();
    Some(expand_environment(&trimmed))
}

pub fn expand_environment(value: &str) -> String {
    let mut result = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(value) => result.push_str(&value),
                    Err(_) => {
                        result.push('%');
                        result.push_str(name);
                        result.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                result.push_str(&rest[start..]);
                return result;
            }
        }
    }
    result.push_str(rest);
    result
}

pub fn extract_product_code(command: &str) -> Option<String> {
    let lowered = command.to_ascii_lowercase();
    let marker = "msiexec";
    let start = lowered.find(marker)?;
    let tail = &command[start..];
    let open = tail.find('{')?;
    let close = tail[open..].find('}')? + open;
    let candidate = &tail[open + 1..close];
    if candidate.len() != 36 {
        return None;
    }
    if !candidate
        .chars()
        .all(|item| item.is_ascii_hexdigit() || item == '-')
    {
        return None;
    }
    Some(candidate.to_ascii_lowercase())
}

fn run_uninstall(product: &InstalledProduct, quiet: bool) -> std::io::Result<i32> {
    let (file, arguments) = match &product.product_code {
        Some(code) => (
            "msiexec.exe".to_string(),
            if quiet {
                format!("/x \"{code}\" /qn /norestart")
            } else {
                format!("/x \"{code}\" /passive /norestart")
            },
        ),
        None => {
            let command = if quiet {
                product
                    .quiet_uninstall_string
                    .clone()
                    .unwrap_or_else(|| product.uninstall_string.clone())
            } else {
                product.uninstall_string.clone()
            };
            let (file, mut arguments) = split_command(&expand_environment(&command));
            if quiet && !has_quiet_switch(&arguments) {
                arguments =
                    append_arguments(&arguments, "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART");
            }
            (file, arguments)
        }
    };
    let status = std::process::Command::new(file).arg(arguments).status()?;
    Ok(status.code().unwrap_or(0))
}

pub fn has_quiet_switch(arguments: &str) -> bool {
    arguments
        .split_whitespace()
        .any(|item| item.eq_ignore_ascii_case("/VERYSILENT") || item.eq_ignore_ascii_case("/S"))
}

pub fn split_command(command: &str) -> (String, String) {
    let value = command.trim();
    if let Some(rest) = value.strip_prefix('"') {
        if let Some(close) = rest.find('"') {
            return (
                rest[..close].to_string(),
                rest[close + 1..].trim_start().to_string(),
            );
        }
    }
    match value.find([' ', '\t']) {
        Some(index) => (
            value[..index].to_string(),
            value[index + 1..].trim_start().to_string(),
        ),
        None => (value.to_string(), String::new()),
    }
}

fn append_arguments(current: &str, addition: &str) -> String {
    if current.trim().is_empty() {
        addition.to_string()
    } else {
        format!("{current} {addition}")
    }
}
