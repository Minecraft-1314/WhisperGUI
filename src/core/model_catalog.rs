use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use crate::core::app_log::AppLog;
use crate::core::app_paths::AppPaths;
use crate::core::error::AppError;
use crate::core::input;
use crate::core::models::ModelSnapshot;
use crate::core::network::{self, HttpsAddress};

const MAX_ID_LENGTH: usize = 32;
const MAX_LABEL_LENGTH: usize = 64;
const MAX_FILE_NAME_LENGTH: usize = 128;

#[derive(Debug, Clone)]
pub struct ModelDefinition {
    pub id: String,
    pub label: String,
    pub file_name: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub url: HttpsAddress,
}

pub struct ModelCatalog {
    search_paths: Vec<PathBuf>,
    models: Mutex<Option<Vec<ModelDefinition>>>,
    last_load_issue: Mutex<Option<String>>,
}

impl Default for ModelCatalog {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl ModelCatalog {
    pub fn new(search_paths: Vec<PathBuf>) -> Self {
        let resolved = match search_paths.is_empty() {
            true => default_search_paths(),
            false => search_paths,
        };
        Self {
            search_paths: resolved,
            models: Mutex::new(None),
            last_load_issue: Mutex::new(None),
        }
    }

    pub fn last_load_issue(&self) -> Option<String> {
        self.last_load_issue
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub fn models(&self) -> Vec<ModelDefinition> {
        if let Ok(guard) = self.models.lock() {
            if let Some(value) = guard.as_ref() {
                return value.clone();
            }
        }
        let loaded = self.load();
        if let Ok(mut guard) = self.models.lock() {
            *guard = Some(loaded.clone());
        }
        loaded
    }

    pub fn find(&self, id: &str) -> Result<ModelDefinition, AppError> {
        self.models()
            .into_iter()
            .find(|model| model.id.eq_ignore_ascii_case(id))
            .ok_or_else(|| {
                AppError::general(format!("Unknown model: {}", input::identifier(Some(id))))
            })
    }

    pub fn snapshots(&self) -> Vec<ModelSnapshot> {
        let cache_directory = AppPaths::model_cache_directory();
        self.models()
            .into_iter()
            .map(|model| {
                let path = cache_directory.join(&model.file_name);
                let cached_bytes = safe_length(&path);
                ModelSnapshot {
                    id: model.id,
                    label: model.label,
                    file_name: model.file_name,
                    size_bytes: model.size_bytes,
                    cached_bytes,
                    is_cached: cached_bytes == model.size_bytes,
                    sha256: model.sha256,
                }
            })
            .collect()
    }

    fn load(&self) -> Vec<ModelDefinition> {
        for path in &self.search_paths {
            if !path.is_file() {
                continue;
            }
            match read_catalog(path) {
                Ok(models) => {
                    self.set_issue(None);
                    return models;
                }
                Err(error) => self.set_issue(Some(AppLog::redact(&error.message()))),
            }
        }
        if self.last_load_issue().is_none() {
            self.set_issue(Some("Model resource is missing.".to_string()));
        }
        fallback_models()
    }

    fn set_issue(&self, issue: Option<String>) {
        if let Ok(mut guard) = self.last_load_issue.lock() {
            *guard = issue;
        }
    }
}

fn read_catalog(path: &Path) -> Result<Vec<ModelDefinition>, AppError> {
    let content = std::fs::read_to_string(path)?;
    let document: Value =
        serde_json::from_str(&content).map_err(|_| AppError::format("Missing models array."))?;
    let array = document
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::format("Missing models array."))?;
    let mut models = Vec::with_capacity(array.len());
    for node in array {
        models.push(parse_model(node)?);
    }
    if models.is_empty() {
        return Err(AppError::format("Model catalog is empty."));
    }
    Ok(models)
}

fn parse_model(node: &Value) -> Result<ModelDefinition, AppError> {
    let invalid = || AppError::format("Invalid model catalog entry.");
    let id = input::text(text_of(node, "id").as_deref(), MAX_ID_LENGTH);
    let label = input::text(text_of(node, "label").as_deref(), MAX_LABEL_LENGTH);
    let file_name = input::text(text_of(node, "fileName").as_deref(), MAX_FILE_NAME_LENGTH);
    let sha256 = input::text(text_of(node, "sha256").as_deref(), 64).to_lowercase();
    let url = network::validate_https(&text_of(node, "url").unwrap_or_default())?;
    let size_bytes = node
        .get("sizeBytes")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or_else(invalid)?;
    let extension_ok = file_name.to_lowercase().ends_with(".pt");
    if id.is_empty()
        || label.is_empty()
        || file_name.is_empty()
        || !extension_ok
        || file_name.contains('\\')
        || file_name.contains('/')
        || sha256.len() != 64
        || !sha256.chars().all(|value| value.is_ascii_hexdigit())
    {
        return Err(invalid());
    }
    let segments = url.directory_segments();
    if segments.len() < 2 || !segments[segments.len() - 2].eq_ignore_ascii_case(&sha256) {
        return Err(invalid());
    }
    Ok(ModelDefinition {
        id,
        label,
        file_name,
        size_bytes,
        sha256,
        url,
    })
}

fn text_of(node: &Value, name: &str) -> Option<String> {
    node.get(name).and_then(Value::as_str).map(str::to_string)
}

fn safe_length(path: &Path) -> i64 {
    std::fs::metadata(path)
        .map(|value| value.len() as i64)
        .unwrap_or(0)
}

fn default_search_paths() -> Vec<PathBuf> {
    vec![
        crate::core::app_paths::executable_directory()
            .join("resources")
            .join("models.json"),
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("resources")
            .join("models.json"),
    ]
}

const FALLBACK: [(&str, &str, &str, i64, &str); 5] = [
    (
        "tiny",
        "Tiny",
        "tiny.pt",
        75572083,
        "65147644a518d12f04e32d6f3b26facc3f8dd46e5390956a9424a650c0ce22b9",
    ),
    (
        "base",
        "Base",
        "base.pt",
        145262807,
        "ed3a0b6b1c0edf879ad9b11b1af5a0e6ab5db9205f891f668f8b0e6c6326e34e",
    ),
    (
        "small",
        "Small",
        "small.pt",
        483617219,
        "9ecf779972d90ba49c06d968637d720dd632c55bbf19d441fb42bf17a411e794",
    ),
    (
        "medium",
        "Medium",
        "medium.pt",
        1528008539,
        "345ae4da62f9b3d59415adc60127b97c714f32e89e936602e85993674d08dcb1",
    ),
    (
        "large",
        "Large",
        "large-v3.pt",
        3087371615,
        "e5b1a55b89c1367dacf97e3e19bfd829a01529dbfdeefa8caeb59b3f1b81dadb",
    ),
];

const OFFICIAL_ROOT: &str = "https://openaipublic.azureedge.net/main/whisper/models";

fn fallback_models() -> Vec<ModelDefinition> {
    FALLBACK
        .iter()
        .map(|(id, label, file_name, size_bytes, sha256)| {
            let url = network::validate_https(&format!("{OFFICIAL_ROOT}/{sha256}/{file_name}"))
                .expect("official whisper model address");
            ModelDefinition {
                id: id.to_string(),
                label: label.to_string(),
                file_name: file_name.to_string(),
                size_bytes: *size_bytes,
                sha256: sha256.to_string(),
                url,
            }
        })
        .collect()
}
