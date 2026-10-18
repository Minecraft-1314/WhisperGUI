use std::path::PathBuf;

pub const MAX_NAME_LENGTH: usize = 80;
pub const MAX_ADDRESS_LENGTH: usize = 2048;
pub const MAX_CREDENTIAL_LENGTH: usize = 8192;
pub const MAX_USER_ID_LENGTH: usize = 128;
pub const MAX_PATH_LENGTH: usize = 32767;

pub fn text(value: Option<&str>, maximum_length: usize) -> String {
    let source = value.unwrap_or("");
    let normalized = source
        .split(['\r', '\n', '\t'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    truncate_chars(&normalized, maximum_length)
}

pub fn path(value: Option<&str>) -> String {
    let normalized = value.unwrap_or("").trim().trim_matches('"').to_string();
    if normalized.chars().count() <= MAX_PATH_LENGTH {
        normalized
    } else {
        String::new()
    }
}

pub fn identifier(value: Option<&str>) -> String {
    text(value, MAX_NAME_LENGTH).to_lowercase()
}

pub fn language(value: &str) -> bool {
    matches!(value, "system" | "zh-CN" | "en-US")
}

pub fn theme(value: &str) -> bool {
    matches!(value, "auto" | "light" | "dark")
}

pub fn device(value: &str) -> bool {
    matches!(value, "auto" | "cpu" | "gpu")
}

pub fn service_type(value: &str) -> bool {
    matches!(value, "http" | "api" | "local")
}

pub fn recognition_language(value: &str) -> bool {
    matches!(
        value,
        "auto" | "zh" | "en" | "ja" | "ko" | "fr" | "de" | "es" | "ru"
    )
}

pub fn clamp_integer(value: &str, minimum: i32, maximum: i32, fallback: i32) -> i32 {
    match value.trim().parse::<i32>() {
        Ok(parsed) => parsed.clamp(minimum, maximum),
        Err(_) => fallback.clamp(minimum, maximum),
    }
}

pub fn full_path(value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

pub fn truncate_chars(value: &str, maximum_length: usize) -> String {
    if value.chars().count() <= maximum_length {
        return value.to_string();
    }
    value.chars().take(maximum_length).collect()
}
