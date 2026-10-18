use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

use crate::core::app_paths::AppPaths;

static GATE: Mutex<()> = Mutex::new(());

pub fn info(message: &str) {
    write("INFO", message);
}

pub fn warn(message: &str) {
    write("WARN", message);
}

pub fn error(message: &str) {
    write("ERROR", message);
}

pub fn redact(message: &str) -> String {
    if message.trim().is_empty() {
        return String::new();
    }
    let value = credential_assignment().replace_all(message, "$1=[REDACTED]");
    let value = bearer().replace_all(&value, "$1 [REDACTED]");
    query_credential()
        .replace_all(&value, "$1=[REDACTED]")
        .into_owned()
}

pub fn utc_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds.div_euclid(86_400);
    let remainder = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.0000000+00:00",
        remainder / 3600,
        (remainder % 3600) / 60,
        remainder % 60
    )
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn write(level: &str, message: &str) {
    let directory = AppPaths::log_directory();
    if AppPaths::ensure_directory(&directory).is_err() {
        return;
    }
    let line = format!("{} {level} {}\r\n", utc_timestamp(), redact(message));
    let Ok(_guard) = GATE.lock() else {
        return;
    };
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(AppPaths::log_path())
    else {
        return;
    };
    let _ = file.write_all(line.as_bytes());
}

fn credential_assignment() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(authorization|token|password|credential|secret|api[-_]?key)\b\s*[:=]\s*(?:bearer\s+)?([^\s,;]+)")
            .expect("credential pattern")
    })
}

fn bearer() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(bearer)\s+([A-Za-z0-9._~+/=-]+)").expect("bearer pattern")
    })
}

fn query_credential() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)([?&](?:token|password|secret|api[-_]?key|credential)=)[^&\s]+")
            .expect("query credential pattern")
    })
}

pub struct AppLog;

impl AppLog {
    pub fn info(message: &str) {
        info(message);
    }

    pub fn warn(message: &str) {
        warn(message);
    }

    pub fn error(message: &str) {
        error(message);
    }

    pub fn redact(message: &str) -> String {
        redact(message)
    }

    pub fn utc_timestamp() -> String {
        utc_timestamp()
    }
}
