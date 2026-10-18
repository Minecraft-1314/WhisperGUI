use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::bridge::json::{number, payload_of, text, text_array};
use crate::bridge::state::StateView;
use crate::bridge::{EventSink, MAX_MESSAGE_CHARACTERS};
use crate::core::app_log::AppLog;
use crate::core::app_paths::{self, AppPaths};
use crate::core::cancel::CancelToken;
use crate::core::credential;
use crate::core::download;
use crate::core::error::AppError;
use crate::core::hardware;
use crate::core::input;
use crate::core::model_catalog::{ModelCatalog, ModelDefinition};
use crate::core::models::{
    DependencyProgress, DownloadProgress, EnvironmentSnapshot, HardwareSnapshot, ServiceProfile,
    TaskSnapshot, TranscriptResult, TranscriptionRequest, TranscriptionUpdate,
};
use crate::core::network;
use crate::core::operation::{OperationGate, OperationLease};
use crate::core::python::PythonEnvironment;
use crate::core::random;
use crate::core::settings::SettingsStore;
use crate::core::transcription::TranscriptionService;
use crate::core::win32;
use crate::core::Progress;

const MAX_FILES: usize = 1_000;
const MAX_SERVICES: usize = 100;
const PROBE_ENVIRONMENT_TIMEOUT: Duration = Duration::from_secs(30);
const AUDIO_EXTENSIONS: [&str; 20] = [
    "mp3", "wav", "m4a", "m4b", "mp4", "flac", "wma", "ogg", "oga", "opus", "aac", "aiff", "aif",
    "webm", "mkv", "mov", "amr", "3gp", "mka", "m4v",
];

pub struct CommandRouter {
    state: StateView,
    operations: OperationGate,
    python: PythonEnvironment,
    transcription: TranscriptionService,
    post_event: EventSink,
    window: std::sync::Mutex<Option<tao::window::WindowId>>,
}

impl CommandRouter {
    pub fn new(post_event: EventSink) -> Arc<Self> {
        let settings = SettingsStore::new(None);
        settings.load();
        let catalog = Arc::new(ModelCatalog::new(Vec::new()));
        let state = StateView::new(settings, Arc::clone(&catalog));
        Arc::new(Self {
            state,
            operations: OperationGate::new(),
            python: PythonEnvironment::new(),
            transcription: TranscriptionService::new(catalog),
            post_event,
            window: std::sync::Mutex::new(None),
        })
    }

    pub fn attach_window(&self, window: &tao::window::Window) {
        if let Ok(mut guard) = self.window.lock() {
            *guard = Some(window.id());
        }
    }

    pub fn handle_message(self: &Arc<Self>, body: String) {
        let parsed = parse_message(&body);
        match parsed {
            Ok((id, action, payload)) => self.execute(id, action, payload),
            Err((id, error)) => {
                AppLog::warn(&format!("Invalid interface message: {}", error.message()));
                self.post_response(id, Err(&error));
            }
        }
    }

    pub fn execute(self: &Arc<Self>, id: i64, action: String, payload: Option<Value>) {
        let router = Arc::clone(self);
        std::thread::spawn(move || match router.dispatch(&action, payload.as_ref()) {
            Ok(value) => router.post_response(id, Ok(&value)),
            Err(error) => router.post_response(id, Err(&error)),
        });
    }

    fn dispatch(
        self: &Arc<Self>,
        action: &str,
        payload: Option<&Value>,
    ) -> Result<Value, AppError> {
        match action {
            "getState" => Ok(self.state.build()),
            "probeEnvironment" => Ok(self.probe_environment()),
            "pickFiles" => Ok(self.pick_files()),
            "pickDirectory" => Ok(self.pick_directory()),
            "saveSettings" => self.save_settings(payload),
            "resetSettings" => self.reset_settings(),
            "importService" => self.import_service(payload),
            "removeService" => self.remove_service(payload),
            "startTranscription" => self.start_transcription(payload),
            "installDependencies" => self.install_dependencies(payload),
            "downloadModel" => self.download_model(payload),
            "cancel" => {
                self.operations.cancel_current();
                Ok(Value::Bool(true))
            }
            "openExternal" => self.open_external(payload),
            "openDirectory" => self.open_directory(payload),
            _ => Err(AppError::validation("Unknown interface action.")),
        }
    }

    fn probe_environment(&self) -> Value {
        let deadline = CancelToken::new().bounded(PROBE_ENVIRONMENT_TIMEOUT);
        let mut hardware = HardwareSnapshot::default();
        if let Ok(value) = std::panic::catch_unwind(|| hardware::detect(&deadline)) {
            hardware = value;
        }
        let mut environment = EnvironmentSnapshot::with_directories(
            AppPaths::runtime_directory().to_string_lossy().into_owned(),
            AppPaths::model_cache_directory()
                .to_string_lossy()
                .into_owned(),
        );
        environment.last_issue = Some("Environment detection timed out.".to_string());
        if let Ok(value) = std::panic::catch_unwind(|| PythonEnvironment::probe(&deadline)) {
            environment = value;
        }
        hardware.has_ffmpeg = environment.has_ffmpeg;
        self.state.set_hardware(hardware);
        self.state.set_environment(environment);
        self.state.build()
    }

    fn pick_files(&self) -> Value {
        let dialog = rfd::FileDialog::new()
            .add_filter("Audio files", &AUDIO_EXTENSIONS)
            .add_filter("All files", &["*"]);
        let selected = self.with_window(dialog, rfd::FileDialog::pick_files);
        let files = selected
            .unwrap_or_default()
            .into_iter()
            .take(MAX_FILES)
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        serde_json::to_value(files).unwrap_or(Value::Array(Vec::new()))
    }

    fn pick_directory(&self) -> Value {
        let dialog = rfd::FileDialog::new().set_directory(app_paths::user_profile());
        let selected = self.with_window(dialog, rfd::FileDialog::pick_folder);
        match selected {
            Some(value) => Value::String(
                input::full_path(&value.to_string_lossy())
                    .to_string_lossy()
                    .into_owned(),
            ),
            None => Value::String(String::new()),
        }
    }

    fn with_window<T>(
        &self,
        dialog: rfd::FileDialog,
        run: impl FnOnce(rfd::FileDialog) -> Option<T>,
    ) -> Option<T> {
        let has_window = self
            .window
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false);
        let _ = has_window;
        run(dialog)
    }

    fn save_settings(&self, payload: Option<&Value>) -> Result<Value, AppError> {
        let store = self.state.settings_store();
        let mut current = store.current();
        let language = text(payload, "language", &current.language);
        let theme = text(payload, "theme", &current.theme);
        let device = text(payload, "device", &current.device);
        let output_directory = input::path(Some(&text(
            payload,
            "outputDirectory",
            current.output_directory.as_deref().unwrap_or(""),
        )));
        if !input::language(&language) || !input::theme(&theme) || !input::device(&device) {
            return Err(AppError::validation("Settings contain an invalid value."));
        }
        if !output_directory.is_empty() && !Path::new(&output_directory).is_dir() {
            return Err(AppError::validation("The output directory does not exist."));
        }
        current.language = language;
        current.theme = theme;
        current.device = device;
        current.output_directory = match output_directory.is_empty() {
            true => None,
            false => Some(output_directory),
        };
        store.save(&current)?;
        Ok(self.state.build())
    }

    fn reset_settings(&self) -> Result<Value, AppError> {
        self.state.settings_store().reset()?;
        Ok(self.state.build())
    }

    fn import_service(&self, payload: Option<&Value>) -> Result<Value, AppError> {
        let store = self.state.settings_store();
        let mut current = store.current();
        if current.services.len() >= MAX_SERVICES {
            return Err(AppError::validation("The service list is full."));
        }
        let name = input::text(Some(&text(payload, "name", "")), input::MAX_NAME_LENGTH);
        let address = text(payload, "address", "").trim().to_string();
        let kind = text(payload, "type", "http").to_lowercase();
        let secret = text(payload, "credential", "");
        let user_id = input::text(
            Some(&text(payload, "userId", "")),
            input::MAX_USER_ID_LENGTH,
        );
        if name.is_empty() {
            return Err(AppError::validation("Name is required."));
        }
        let validated = network::validate_https(&address)?;
        if !input::service_type(&kind) {
            return Err(AppError::validation("Service type is invalid."));
        }
        if secret.chars().count() > input::MAX_CREDENTIAL_LENGTH {
            return Err(AppError::validation("Credential is too long."));
        }
        current.services.push(ServiceProfile {
            id: random::hex(8),
            name,
            address: validated.canonical().to_string(),
            kind,
            user_id,
            credential_cipher: credential::protect(&secret),
        });
        store.save(&current)?;
        Ok(self.state.build())
    }

    fn remove_service(&self, payload: Option<&Value>) -> Result<Value, AppError> {
        let store = self.state.settings_store();
        let mut current = store.current();
        let id = input::identifier(Some(&text(payload, "id", "")));
        let before = current.services.len();
        current.services.retain(|service| service.id != id);
        if current.services.len() == before {
            return Err(AppError::validation("Service does not exist."));
        }
        store.save(&current)?;
        Ok(self.state.build())
    }

    fn start_transcription(self: &Arc<Self>, payload: Option<&Value>) -> Result<Value, AppError> {
        let files = text_array(payload, "files", MAX_FILES).map_err(AppError::validation)?;
        let model_id = input::text(Some(&text(payload, "model", "")), 32);
        let device = input::text(Some(&text(payload, "device", "")), 8);
        let language = input::text(Some(&text(payload, "language", "")), 8);
        let output_directory = input::path(Some(&text(payload, "outputDirectory", "")));
        let model = self.state.catalog().find(&model_id)?;
        let request = TranscriptionRequest {
            files,
            model: model.id.clone(),
            model_file: model.file_name.clone(),
            device,
            output_directory,
            language,
            cache_directory: AppPaths::model_cache_directory()
                .to_string_lossy()
                .into_owned(),
        };
        self.transcription.validate(&request)?;
        let id = random::guid();
        let task_id = id.clone();
        self.begin_task(&id, "transcription", move |router, lease| {
            let sink = router.post_event.clone();
            let progress_sink = sink.clone();
            let log_sink = sink.clone();
            let task = task_id.clone();
            let progress = move |update: TranscriptionUpdate| {
                progress_sink(json!({
                    "type": "transcription",
                    "taskId": task,
                    "updateType": update.kind,
                    "current": update.current,
                    "total": update.total,
                    "text": update.text,
                    "path": update.path,
                    "error": update.error,
                }));
            };
            let log: crate::core::process::LogSink = Arc::new(move |message: String| {
                log_sink(json!({
                    "type": "log",
                    "message": AppLog::redact(&message),
                }));
            });
            let result: TranscriptResult =
                router
                    .transcription
                    .run(&request, &progress, &log, &lease.token());
            sink(json!({
                "type": "transcriptionComplete",
                "taskId": task_id,
                "outputFiles": result.output_files,
                "failedFiles": result.failed_files,
                "cancelled": result.cancelled,
                "error": result.error,
            }));
        })
    }

    fn install_dependencies(self: &Arc<Self>, payload: Option<&Value>) -> Result<Value, AppError> {
        let target = input::text(Some(&text(payload, "target", "")), 8);
        if !matches!(target.as_str(), "cpu" | "gpu") {
            return Err(AppError::validation("Install target is invalid."));
        }
        let id = random::guid();
        let task_id = id.clone();
        self.begin_task(&id, "install", move |router, lease| {
            let sink = router.post_event.clone();
            let log_sink = sink.clone();
            let progress_sink = sink.clone();
            let task = task_id.clone();
            let log: crate::core::process::LogSink = Arc::new(move |message: String| {
                log_sink(json!({
                    "type": "log",
                    "message": AppLog::redact(&message),
                }));
            });
            let progress = move |update: DependencyProgress| {
                progress_sink(json!({
                    "type": "installProgress",
                    "taskId": task,
                    "phase": update.phase,
                    "percent": update.percent,
                    "message": update.message,
                }));
            };
            match router
                .python
                .install(&target, &log, &progress, &lease.token())
            {
                Ok(()) => {
                    let environment = PythonEnvironment::probe(&lease.token());
                    router.state.set_environment(environment);
                    sink(json!({
                        "type": "installComplete",
                        "taskId": task_id,
                        "target": target,
                        "ok": true,
                    }));
                }
                Err(error) => {
                    sink(json!({
                        "type": "operationError",
                        "taskId": task_id,
                        "kind": "install",
                        "message": error.message(),
                    }));
                }
            }
        })
    }

    fn download_model(self: &Arc<Self>, payload: Option<&Value>) -> Result<Value, AppError> {
        let model_id = input::text(Some(&text(payload, "model", "")), 32);
        let model: ModelDefinition = self.state.catalog().find(&model_id)?;
        let id = random::guid();
        let task_id = id.clone();
        self.begin_task(&id, "download", move |router, lease| {
            let sink = router.post_event.clone();
            let progress_sink = sink.clone();
            let task = task_id.clone();
            let name = model.id.clone();
            let progress: Progress<DownloadProgress> = Arc::new(move |update: DownloadProgress| {
                progress_sink(json!({
                    "type": "downloadProgress",
                    "taskId": task,
                    "model": name,
                    "receivedBytes": update.received_bytes,
                    "totalBytes": update.total_bytes,
                    "percent": update.percent,
                }));
            });
            match download::download(&model, Some(&progress), &lease.token()) {
                Ok(path) => {
                    sink(json!({
                        "type": "downloadComplete",
                        "taskId": task_id,
                        "model": model.id,
                        "path": path,
                        "ok": true,
                    }));
                }
                Err(error) => {
                    sink(json!({
                        "type": "operationError",
                        "taskId": task_id,
                        "kind": "download",
                        "message": error.message(),
                    }));
                }
            }
        })
    }

    fn open_external(&self, payload: Option<&Value>) -> Result<Value, AppError> {
        let url = network::validate_https(&text(payload, "url", ""))?;
        win32::open_external(url.canonical()).map_err(AppError::general)?;
        Ok(Value::Bool(true))
    }

    fn open_directory(&self, payload: Option<&Value>) -> Result<Value, AppError> {
        let normalized = input::full_path(&input::path(Some(&text(payload, "path", ""))));
        if !normalized.is_absolute() || !normalized.is_dir() {
            return Err(AppError::general("Directory does not exist."));
        }
        win32::open_external(&normalized.to_string_lossy()).map_err(AppError::general)?;
        Ok(Value::Bool(true))
    }

    fn begin_task(
        self: &Arc<Self>,
        id: &str,
        kind: &str,
        body: impl FnOnce(&CommandRouter, OperationLease) + Send + 'static,
    ) -> Result<Value, AppError> {
        let lease = self.operations.create()?;
        let running = TaskSnapshot::running(id, kind);
        self.state.set_task(running.clone());
        (self.post_event)(task_event(&running));
        let router = Arc::clone(self);
        let task_id = id.to_string();
        let task_kind = kind.to_string();
        let started_id = task_id.clone();
        std::thread::spawn(move || {
            let token = lease.token();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                body(&router, lease);
            }));
            let (state, message) = match outcome {
                Ok(()) => match token.is_cancelled() {
                    true => ("cancelled", String::new()),
                    false => ("completed", String::new()),
                },
                Err(_) => (
                    "failed",
                    "Operation failed. Details were written to the local log.".to_string(),
                ),
            };
            let completed = TaskSnapshot::finished(&task_id, &task_kind, state, &message);
            router.state.set_task(completed.clone());
            (router.post_event)(task_event(&completed));
        });
        Ok(json!({ "taskId": started_id, "started": true }))
    }

    fn post_response(&self, id: i64, result: Result<&Value, &AppError>) {
        let payload = match result {
            Ok(value) => json!({ "id": id, "ok": true, "result": value, "error": Value::Null }),
            Err(error) => json!({
                "id": id,
                "ok": false,
                "result": Value::Null,
                "error": error.message(),
            }),
        };
        (self.post_event)(payload);
    }
}

fn task_event(snapshot: &TaskSnapshot) -> Value {
    json!({ "type": "task", "task": snapshot })
}

fn parse_message(body: &str) -> Result<(i64, String, Option<Value>), (i64, AppError)> {
    if body.chars().count() > MAX_MESSAGE_CHARACTERS {
        return Err((
            0,
            AppError::validation("Interface message exceeds the size limit."),
        ));
    }
    let root: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => {
            return Err((
                0,
                AppError::validation("Interface message format is invalid."),
            ))
        }
    };
    let id = number(Some(&root), "id", 0);
    let action = text(Some(&root), "action", "");
    if action.is_empty() || action.len() > 64 {
        return Err((id, AppError::validation("Interface action is invalid.")));
    }
    Ok((id, action, payload_of(&root).cloned()))
}
