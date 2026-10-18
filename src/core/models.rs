use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub version: i32,
    pub language: String,
    pub theme: String,
    pub device: String,
    pub output_directory: Option<String>,
    pub services: Vec<ServiceProfile>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: 1,
            language: "system".to_string(),
            theme: "auto".to_string(),
            device: "auto".to_string(),
            output_directory: None,
            services: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServiceProfile {
    pub id: String,
    pub name: String,
    pub address: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub user_id: String,
    pub credential_cipher: String,
}

impl Default for ServiceProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            address: String::new(),
            kind: "http".to_string(),
            user_id: String::new(),
            credential_cipher: String::new(),
        }
    }
}

impl ServiceProfile {
    pub fn has_credential(&self) -> bool {
        !self.credential_cipher.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ServiceProfileDraft {
    pub name: String,
    pub address: String,
    pub kind: String,
    pub credential: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HardwareSnapshot {
    pub has_cuda: bool,
    pub has_mps: bool,
    pub gpu_name: String,
    pub vram_gb: f64,
    pub has_ffmpeg: bool,
    pub is_administrator: bool,
    pub last_issue: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EnvironmentSnapshot {
    pub has_system_python: bool,
    pub has_local_runtime: bool,
    pub has_whisper: bool,
    pub has_torch: bool,
    pub has_ffmpeg: bool,
    pub python_version: String,
    pub runtime_directory: String,
    pub cache_directory: String,
    pub last_issue: Option<String>,
}

impl EnvironmentSnapshot {
    pub fn with_directories(runtime_directory: String, cache_directory: String) -> Self {
        Self {
            runtime_directory,
            cache_directory,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSnapshot {
    pub id: String,
    pub label: String,
    pub file_name: String,
    pub size_bytes: i64,
    pub cached_bytes: i64,
    pub is_cached: bool,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub progress: i32,
    pub total: i32,
    pub message: String,
}

impl TaskSnapshot {
    pub fn idle() -> Self {
        Self {
            id: String::new(),
            kind: String::new(),
            state: "idle".to_string(),
            progress: 0,
            total: 0,
            message: String::new(),
        }
    }

    pub fn running(id: &str, kind: &str) -> Self {
        Self {
            id: id.to_string(),
            kind: kind.to_string(),
            state: "running".to_string(),
            progress: 0,
            total: 0,
            message: String::new(),
        }
    }

    pub fn finished(id: &str, kind: &str, state: &str, message: &str) -> Self {
        Self {
            id: id.to_string(),
            kind: kind.to_string(),
            state: state.to_string(),
            progress: 0,
            total: 0,
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptionRequest {
    pub files: Vec<String>,
    pub model: String,
    pub model_file: String,
    pub device: String,
    pub output_directory: String,
    pub language: String,
    pub cache_directory: String,
}

#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub output_files: Vec<String>,
    pub failed_files: Vec<String>,
    pub cancelled: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TranscriptionUpdate {
    pub kind: String,
    pub current: i32,
    pub total: i32,
    pub text: String,
    pub path: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub received_bytes: i64,
    pub total_bytes: i64,
    pub percent: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyProgress {
    pub phase: String,
    pub percent: i32,
    pub message: String,
}

impl DependencyProgress {
    pub fn new(phase: &str, percent: i32, message: &str) -> Self {
        Self {
            phase: phase.to_string(),
            percent,
            message: message.to_string(),
        }
    }
}
