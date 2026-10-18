use std::time::Duration;

use crate::core::cancel::CancelToken;
use crate::core::models::HardwareSnapshot;
use crate::core::process;
use crate::core::win32;

const NVIDIA_SMI_TIMEOUT: Duration = Duration::from_secs(15);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const MPS_MEMORY_GB: f64 = 6.0;

const CUDA_VARIANTS: [(&str, i32, i32); 5] = [
    ("cu128", 570, 0),
    ("cu126", 560, 0),
    ("cu124", 550, 0),
    ("cu121", 530, 0),
    ("cu118", 520, 0),
];

pub fn detect(cancellation: &CancelToken) -> HardwareSnapshot {
    let (has_cuda, mut gpu_name, mut vram_gb, issue) = detect_cuda(cancellation);
    let has_mps = cfg!(target_os = "macos");
    if has_mps && vram_gb <= 0.0 {
        vram_gb = MPS_MEMORY_GB;
        gpu_name = "Apple MPS".to_string();
    }
    let has_ffmpeg = has_command("ffmpeg", cancellation);
    HardwareSnapshot {
        has_cuda,
        has_mps,
        gpu_name,
        vram_gb,
        has_ffmpeg,
        is_administrator: win32::is_administrator(),
        last_issue: match issue.is_empty() {
            true => None,
            false => Some(issue),
        },
    }
}

pub fn detect_ffmpeg(cancellation: &CancelToken) -> (bool, String) {
    let output = process::capture(
        "ffmpeg",
        &["-version".to_string()],
        bounded(cancellation, COMMAND_TIMEOUT),
    );
    if !output.success {
        return (false, String::new());
    }
    match version_of(&output.text) {
        Some(value) => (true, value),
        None => (true, "unknown".to_string()),
    }
}

pub fn detect_cuda_driver_version(cancellation: &CancelToken) -> Option<String> {
    let output = process::capture(
        "nvidia-smi",
        &[
            "--query-gpu=driver_version".to_string(),
            "--format=csv,noheader".to_string(),
        ],
        bounded(cancellation, NVIDIA_SMI_TIMEOUT),
    );
    if !output.success {
        return None;
    }
    driver_of(&output.text)
}

pub fn supported_cuda_variants(driver_version: Option<&str>) -> Vec<String> {
    let (major, minor) = parse_driver(driver_version);
    if major < 0 {
        return CUDA_VARIANTS
            .iter()
            .map(|item| item.0.to_string())
            .collect();
    }
    CUDA_VARIANTS
        .iter()
        .filter(|item| (major, minor) >= (item.1, item.2))
        .map(|item| item.0.to_string())
        .collect()
}

fn detect_cuda(cancellation: &CancelToken) -> (bool, String, f64, String) {
    let output = process::capture(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total".to_string(),
            "--format=csv,noheader,nounits".to_string(),
        ],
        bounded(cancellation, NVIDIA_SMI_TIMEOUT),
    );
    if !output.success {
        return (false, String::new(), 0.0, String::new());
    }
    let first_line = output
        .text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let Some(separator) = first_line.rfind(',') else {
        return (
            false,
            String::new(),
            0.0,
            "GPU detection output could not be parsed.".to_string(),
        );
    };
    let name = first_line[..separator].trim().to_string();
    let Ok(megabytes) = first_line[separator + 1..].trim().parse::<f64>() else {
        return (
            false,
            String::new(),
            0.0,
            "GPU memory detection output could not be parsed.".to_string(),
        );
    };
    (true, name, megabytes / 1024.0, String::new())
}

fn has_command(command: &str, cancellation: &CancelToken) -> bool {
    process::capture(
        "where",
        &[command.to_string()],
        bounded(cancellation, COMMAND_TIMEOUT),
    )
    .success
}

fn bounded(cancellation: &CancelToken, duration: Duration) -> CancelToken {
    cancellation.bounded(duration)
}

fn version_of(output: &str) -> Option<String> {
    let marker = "ffmpeg version ";
    let start = output.find(marker)? + marker.len();
    let rest = &output[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn driver_of(output: &str) -> Option<String> {
    let bytes = output.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_digit() {
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'.' {
                index += 1;
                let minor_start = index;
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
                if index > minor_start {
                    return Some(output[start..index].to_string());
                }
            }
        } else {
            index += 1;
        }
    }
    None
}

fn parse_driver(value: Option<&str>) -> (i32, i32) {
    let Some(found) = value.and_then(driver_of) else {
        return (-1, -1);
    };
    let Some((major, minor)) = found.split_once('.') else {
        return (-1, -1);
    };
    match (major.parse::<i32>(), minor.parse::<i32>()) {
        (Ok(major), Ok(minor)) => (major, minor),
        _ => (-1, -1),
    }
}
