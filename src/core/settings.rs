use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::core::app_log::AppLog;
use crate::core::app_paths::AppPaths;
use crate::core::credential;
use crate::core::error::AppError;
use crate::core::input;
use crate::core::models::{AppSettings, ServiceProfile};
use crate::core::network;
use crate::core::random;

const MAX_SERVICES: usize = 100;

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<AppSettings>,
    last_load_issue: Mutex<Option<String>>,
}

impl SettingsStore {
    pub fn new(path: Option<PathBuf>) -> Self {
        let resolved = match path {
            Some(value) if !value.as_os_str().is_empty() => value,
            _ => AppPaths::settings_path(),
        };
        Self {
            path: resolved,
            current: Mutex::new(AppSettings::default()),
            last_load_issue: Mutex::new(None),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn path_string(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    pub fn last_load_issue(&self) -> Option<String> {
        self.last_load_issue
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub fn load(&self) -> AppSettings {
        if !self.exists() {
            self.replace_current(AppSettings::default(), None);
            return self.current();
        }
        let content = match fs::read_to_string(&self.path) {
            Ok(value) => value,
            Err(error) => {
                self.replace_current(
                    AppSettings::default(),
                    Some(AppLog::redact(&error.to_string())),
                );
                return self.current();
            }
        };
        match serde_json::from_str::<AppSettings>(&content) {
            Ok(parsed) if parsed.version == 1 => {
                self.replace_current(sanitize(parsed), None);
            }
            Ok(_) => {
                self.replace_current(
                    AppSettings::default(),
                    Some("Unsupported or invalid settings were safely ignored.".to_string()),
                );
            }
            Err(_) => {
                self.replace_current(
                    AppSettings::default(),
                    Some("Damaged settings were safely ignored.".to_string()),
                );
            }
        }
        self.current()
    }

    pub fn save(&self, settings: &AppSettings) -> Result<(), AppError> {
        let sanitized = sanitize(settings.clone());
        let directory = self
            .path
            .parent()
            .filter(|value| !value.as_os_str().is_empty())
            .ok_or_else(|| AppError::general("Settings directory is invalid."))?
            .to_path_buf();
        AppPaths::ensure_directory(&directory)
            .map_err(|error| AppError::general(AppLog::redact(&error.to_string())))?;
        let mut temporary = self.path.clone().into_os_string();
        temporary.push(".tmp");
        let temporary_path = PathBuf::from(temporary);
        let json = serde_json::to_string_pretty(&sanitized)
            .map_err(|error| AppError::general(error.to_string()))?;
        let result = (|| -> Result<(), AppError> {
            fs::write(&temporary_path, json.as_bytes())
                .map_err(|error| AppError::general(AppLog::redact(&error.to_string())))?;
            fs::rename(&temporary_path, &self.path)
                .map_err(|error| AppError::general(AppLog::redact(&error.to_string())))?;
            Ok(())
        })();
        let _ = fs::remove_file(&temporary_path);
        result?;
        self.replace_current(sanitized, None);
        Ok(())
    }

    pub fn reset(&self) -> Result<(), AppError> {
        if self.exists() {
            fs::remove_file(&self.path)
                .map_err(|error| AppError::general(format!("Unable to reset settings. {error}")))?;
        }
        self.replace_current(AppSettings::default(), None);
        Ok(())
    }

    pub fn current(&self) -> AppSettings {
        self.current
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    fn replace_current(&self, settings: AppSettings, issue: Option<String>) {
        if let Ok(mut guard) = self.current.lock() {
            *guard = settings;
        }
        if let Ok(mut guard) = self.last_load_issue.lock() {
            *guard = issue;
        }
    }
}

pub fn sanitize(settings: AppSettings) -> AppSettings {
    let mut result = AppSettings {
        version: 1,
        language: match input::language(&settings.language) {
            true => settings.language,
            false => "zh-CN".to_string(),
        },
        theme: match input::theme(&settings.theme) {
            true => settings.theme,
            false => "auto".to_string(),
        },
        device: match input::device(&settings.device) {
            true => settings.device,
            false => "auto".to_string(),
        },
        output_directory: match settings.output_directory {
            Some(value) if !value.trim().is_empty() => {
                let normalized = input::path(Some(&value));
                match normalized.is_empty() {
                    true => None,
                    false => Some(normalized),
                }
            }
            _ => None,
        },
        services: Vec::new(),
    };

    let mut identifiers: Vec<String> = Vec::new();
    for profile in settings.services.into_iter().take(MAX_SERVICES) {
        let name = input::text(Some(&profile.name), input::MAX_NAME_LENGTH);
        if name.is_empty() {
            continue;
        }
        let Ok(address) = network::validate_https(&profile.address) else {
            continue;
        };
        if !input::service_type(&profile.kind) {
            continue;
        }
        let mut id = input::identifier(Some(&profile.id));
        if id.is_empty() || identifiers.contains(&id) {
            id = random::hex(8);
        }
        identifiers.push(id.clone());
        let mut cipher = profile.credential_cipher;
        if !cipher.is_empty() && !credential::is_valid_cipher(&cipher) {
            cipher = String::new();
        }
        result.services.push(ServiceProfile {
            id,
            name,
            address: address.canonical().to_string(),
            kind: profile.kind,
            user_id: input::text(Some(&profile.user_id), input::MAX_USER_ID_LENGTH),
            credential_cipher: cipher,
        });
    }
    result
}
