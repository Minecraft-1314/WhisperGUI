use std::path::{Path, PathBuf};

pub struct AppPaths;

impl AppPaths {
    pub fn data_directory() -> PathBuf {
        local_app_data().join("WhisperGUI")
    }

    pub fn settings_path() -> PathBuf {
        Self::data_directory().join("settings.json")
    }

    pub fn runtime_directory() -> PathBuf {
        Self::data_directory().join("runtime")
    }

    pub fn log_directory() -> PathBuf {
        Self::data_directory().join("logs")
    }

    pub fn log_path() -> PathBuf {
        Self::log_directory().join("app.log")
    }

    pub fn webview_directory() -> PathBuf {
        Self::data_directory().join("WebView2")
    }

    pub fn webview_data_directory() -> PathBuf {
        let primary = Self::webview_directory();
        match Self::ensure_directory(&primary) {
            Ok(_) => primary,
            Err(error) => {
                crate::core::app_log::AppLog::warn(&format!(
                    "Falling back to temporary WebView2 data: {}",
                    error
                ));
                let fallback = std::env::temp_dir().join("WhisperGUI").join("WebView2");
                let _ = Self::ensure_directory(&fallback);
                fallback
            }
        }
    }

    pub fn executable_directory() -> PathBuf {
        executable_directory()
    }

    pub fn model_cache_directory() -> PathBuf {
        let root = match std::env::var("XDG_CACHE_HOME") {
            Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => user_profile().join(".cache"),
        };
        root.join("whisper")
    }

    pub fn ensure_directory(path: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(path)?;
        Ok(path.to_path_buf())
    }
}

pub fn user_profile() -> PathBuf {
    match std::env::var("USERPROFILE") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

pub fn executable_directory() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|value| value.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn local_app_data() -> PathBuf {
    match std::env::var("LOCALAPPDATA") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => user_profile().join("AppData").join("Local"),
    }
}
