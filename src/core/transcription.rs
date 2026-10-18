use std::collections::HashSet;
use std::path::Path;

use crate::core::app_log::AppLog;
use crate::core::cancel::CancelToken;
use crate::core::error::AppError;
use crate::core::input;
use std::sync::Arc;

use crate::core::model_catalog::ModelCatalog;
use crate::core::models::{TranscriptResult, TranscriptionRequest, TranscriptionUpdate};
use crate::core::process::LogSink;
use crate::core::python::PythonEnvironment;

const MAXIMUM_FILES: usize = 1000;

const AUDIO_EXTENSIONS: [&str; 20] = [
    "mp3", "wav", "m4a", "m4b", "mp4", "flac", "wma", "ogg", "oga", "opus", "aac", "aiff", "aif",
    "webm", "mkv", "mov", "amr", "3gp", "mka", "m4v",
];

pub struct TranscriptionService {
    catalog: Arc<ModelCatalog>,
}

impl TranscriptionService {
    pub fn new(catalog: Arc<ModelCatalog>) -> Self {
        Self { catalog }
    }

    pub fn run(
        &self,
        request: &TranscriptionRequest,
        progress: &dyn Fn(TranscriptionUpdate),
        log: &LogSink,
        cancellation: &CancelToken,
    ) -> TranscriptResult {
        let mut saved_files: Vec<String> = Vec::new();
        let mut failed_files: Vec<String> = Vec::new();
        let environment = PythonEnvironment::probe(cancellation);
        if !environment.has_local_runtime || !environment.has_whisper || !environment.has_torch {
            return TranscriptResult {
                output_files: saved_files,
                failed_files,
                cancelled: false,
                error: Some("Python dependencies are not ready.".to_string()),
            };
        }
        let needs_ffmpeg = request
            .files
            .iter()
            .any(|path| !path.to_lowercase().ends_with(".wav"));
        if !environment.has_ffmpeg && needs_ffmpeg {
            return TranscriptResult {
                output_files: saved_files,
                failed_files,
                cancelled: false,
                error: Some("ffmpeg is required for the selected audio files.".to_string()),
            };
        }

        let mut child = match PythonEnvironment::start_transcription(request, cancellation) {
            Ok(value) => value,
            Err(error) => {
                return TranscriptResult {
                    output_files: saved_files,
                    failed_files,
                    cancelled: matches!(error, AppError::Cancelled),
                    error: match matches!(error, AppError::Cancelled) {
                        true => None,
                        false => Some(error.message()),
                    },
                }
            }
        };

        loop {
            if cancellation.is_cancelled() {
                child.terminate();
                return TranscriptResult {
                    output_files: saved_files,
                    failed_files,
                    cancelled: true,
                    error: None,
                };
            }
            let Some(line) = child.read_line() else {
                break;
            };
            if line.is_empty() {
                continue;
            }
            let Ok(root) = serde_json::from_str::<serde_json::Value>(&line) else {
                child.terminate();
                return TranscriptResult {
                    output_files: saved_files,
                    failed_files,
                    cancelled: false,
                    error: Some("Unknown transcription event.".to_string()),
                };
            };
            match root
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
            {
                "log" => log(read_string(&root, "message")),
                "progress" => progress(TranscriptionUpdate {
                    kind: "progress".to_string(),
                    current: read_int(&root, "current"),
                    total: read_int(&root, "total"),
                    text: read_string(&root, "message"),
                    path: String::new(),
                    error: None,
                }),
                "result" | "language" | "saved" | "fileFailed" => {
                    let kind = root
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let path = read_string(&root, "path");
                    if kind == "saved" {
                        saved_files.push(path.clone());
                    } else if kind == "fileFailed" {
                        failed_files.push(path.clone());
                    }
                    progress(TranscriptionUpdate {
                        kind,
                        current: read_int(&root, "current"),
                        total: read_int(&root, "total"),
                        text: read_string(&root, "message"),
                        path,
                        error: read_nullable_string(&root, "error"),
                    });
                }
                "summary" => {
                    return TranscriptResult {
                        output_files: read_string_array(&root, "outputFiles"),
                        failed_files: read_string_array(&root, "failedFiles"),
                        cancelled: root
                            .get("cancelled")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                        error: read_nullable_string(&root, "error"),
                    }
                }
                _ => {
                    child.terminate();
                    return TranscriptResult {
                        output_files: saved_files,
                        failed_files,
                        cancelled: false,
                        error: Some("Unknown transcription event.".to_string()),
                    };
                }
            }
        }

        let code = child.wait();
        match code {
            Some(0) => TranscriptResult {
                output_files: saved_files,
                failed_files,
                cancelled: false,
                error: None,
            },
            Some(value) => TranscriptResult {
                output_files: saved_files,
                failed_files,
                cancelled: false,
                error: Some(format!("Transcription process exited with code {value}.")),
            },
            None => TranscriptResult {
                output_files: saved_files,
                failed_files,
                cancelled: false,
                error: Some("Transcription process did not exit cleanly.".to_string()),
            },
        }
    }

    pub fn validate(&self, request: &TranscriptionRequest) -> Result<(), AppError> {
        if request.files.is_empty() || request.files.len() > MAXIMUM_FILES {
            return Err(AppError::validation(
                "Select between 1 and 1000 audio files.",
            ));
        }
        if !input::device(&request.device) {
            return Err(AppError::validation("The selected device is invalid."));
        }
        if !input::recognition_language(&request.language) {
            return Err(AppError::validation("The selected language is invalid."));
        }
        if request.output_directory.trim().is_empty()
            || !Path::new(&request.output_directory).is_dir()
        {
            return Err(AppError::validation("The output directory does not exist."));
        }

        let model = self.catalog.find(&request.model)?;
        if !request.model_file.eq_ignore_ascii_case(&model.file_name) {
            return Err(AppError::validation(
                "The model file does not match the model catalog.",
            ));
        }
        let model_path = Path::new(&request.cache_directory).join(&request.model_file);
        let ready = std::fs::metadata(&model_path)
            .map(|value| value.len() as i64 == model.size_bytes)
            .unwrap_or(false);
        if !ready {
            return Err(AppError::model_not_ready(model.id));
        }

        let mut unique: HashSet<String> = HashSet::new();
        for path in &request.files {
            let normalized = input::full_path(path);
            let extension = normalized
                .extension()
                .map(|value| value.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let usable = normalized.is_file()
                && normalized.is_absolute()
                && AUDIO_EXTENSIONS.contains(&extension.as_str())
                && unique.insert(normalized.to_string_lossy().into_owned());
            if !usable {
                return Err(AppError::validation(
                    "One or more selected audio files are invalid.",
                ));
            }
        }
        Ok(())
    }
}

fn read_string(root: &serde_json::Value, name: &str) -> String {
    root.get(name)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn read_nullable_string(root: &serde_json::Value, name: &str) -> Option<String> {
    root.get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn read_int(root: &serde_json::Value, name: &str) -> i32 {
    root.get(name)
        .and_then(serde_json::Value::as_i64)
        .map(|value| value as i32)
        .unwrap_or(0)
}

fn read_string_array(root: &serde_json::Value, name: &str) -> Vec<String> {
    root.get(name)
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn redact_error(error: &str) -> String {
    AppLog::redact(error)
}
