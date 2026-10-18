use std::path::Path;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Shell::{IsUserAnAdmin, ShellExecuteW};

const SW_SHOWNORMAL: i32 = 1;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

pub fn is_administrator() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

pub fn open_external(target: &str) -> Result<(), String> {
    let operation = wide("open");
    let value = wide(target);
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut::<core::ffi::c_void>() as HWND,
            operation.as_ptr(),
            value.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        return Err("Unable to open the external target.".to_string());
    }
    Ok(())
}

pub fn free_space_bytes(path: &Path) -> Option<u64> {
    let value = wide(&path.to_string_lossy());
    let mut available = 0u64;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            value.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    match ok {
        0 => None,
        _ => Some(available),
    }
}

pub fn show_error(title: &str, message: &str) {
    let heading = wide(title);
    let body = wide(message);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut::<core::ffi::c_void>() as HWND,
            body.as_ptr(),
            heading.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
