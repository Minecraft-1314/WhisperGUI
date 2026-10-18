use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::core::app_log::AppLog;
use crate::core::app_paths::AppPaths;
use crate::core::cancel::CancelToken;
use crate::core::error::AppError;
use crate::core::hardware;
use crate::core::models::{DependencyProgress, EnvironmentSnapshot, TranscriptionRequest};
use crate::core::process::{self, TranscriptionProcess};

pub use crate::core::process::LogSink;

const PYTHON_PROBE_TIMEOUT: Duration = Duration::from_secs(20);
const IMPORT_PROBE_TIMEOUT: Duration = Duration::from_secs(900);
const VENV_TIMEOUT: Duration = Duration::from_secs(120);
const PACKAGE_INSTALL_TIMEOUT: Duration = Duration::from_secs(1800);
const TORCH_INSTALL_TIMEOUT: Duration = Duration::from_secs(3600);
const MAXIMUM_INSTALL_ATTEMPTS: usize = 2;
const WHISPER_PACKAGE: &str = "openai-whisper>=20231117";
const TORCH_PACKAGE: &str = "torch";
const TORCH_PACKAGES: [&str; 3] = ["torch", "torchvision", "torchaudio"];
const PYTHON_CANDIDATES: [&str; 3] = ["python.exe", "python3.exe", "py.exe"];

pub struct PythonEnvironment {
    install_gate: Mutex<()>,
}

impl Default for PythonEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonEnvironment {
    pub fn new() -> Self {
        Self {
            install_gate: Mutex::new(()),
        }
    }

    pub fn runtime_directory() -> PathBuf {
        AppPaths::runtime_directory()
    }

    pub fn local_python_path() -> PathBuf {
        Self::runtime_directory().join("Scripts").join("python.exe")
    }

    pub fn probe(cancellation: &CancelToken) -> EnvironmentSnapshot {
        let (system_python, system_version) = find_system_python(cancellation);
        let has_local_runtime = Self::local_python_path().is_file();
        let python_path = match has_local_runtime {
            true => Self::local_python_path(),
            false => match system_python.as_ref() {
                Some(value) => PathBuf::from(value),
                None => PathBuf::new(),
            },
        };
        let mut has_whisper = false;
        let mut has_torch = false;
        let mut python_version = system_version;
        if has_local_runtime {
            let output = process::capture(
                &Self::local_python_path().to_string_lossy(),
                &["--version".to_string()],
                cancellation.bounded(PYTHON_PROBE_TIMEOUT),
            );
            python_version = match output.success {
                true => output.text.trim().to_string(),
                false => String::new(),
            };
        }
        if !python_path.as_os_str().is_empty() {
            let (whisper, torch) = probe_specs(&python_path.to_string_lossy(), cancellation);
            has_whisper = whisper;
            has_torch = torch;
        }
        let (has_ffmpeg, _) = hardware::detect_ffmpeg(cancellation);
        let mut snapshot = EnvironmentSnapshot {
            has_system_python: system_python.is_some(),
            has_local_runtime,
            has_whisper,
            has_torch,
            has_ffmpeg,
            python_version,
            runtime_directory: Self::runtime_directory().to_string_lossy().into_owned(),
            cache_directory: AppPaths::model_cache_directory()
                .to_string_lossy()
                .into_owned(),
            last_issue: None,
        };
        if !snapshot.has_system_python && !has_local_runtime {
            snapshot.last_issue = Some("Python 3 was not found on this system.".to_string());
        }
        snapshot
    }

    pub fn install(
        &self,
        target_device: &str,
        log: &LogSink,
        progress: &dyn Fn(DependencyProgress),
        cancellation: &CancelToken,
    ) -> Result<(), AppError> {
        if !matches!(target_device, "cpu" | "gpu") {
            return Err(AppError::python("Unsupported device target."));
        }
        let Ok(_guard) = self.install_gate.lock() else {
            return Err(AppError::python("The dependency installer is unavailable."));
        };
        progress(DependencyProgress::new(
            "checking",
            5,
            "Checking local runtime...",
        ));
        let python_path = ensure_runtime(log, cancellation)?;
        progress(DependencyProgress::new(
            "runtime",
            15,
            "Local Python environment is ready.",
        ));
        let has_torch = has_spec(&python_path, "torch", cancellation);
        let has_cuda = has_torch && torch_has_cuda(&python_path, cancellation);
        let already_matches = (target_device == "cpu" && has_torch && !has_cuda)
            || (target_device == "gpu" && has_torch && has_cuda);
        if already_matches {
            log(match target_device {
                "cpu" => "PyTorch CPU is already available.".to_string(),
                _ => "PyTorch CUDA is already available.".to_string(),
            });
            progress(DependencyProgress::new(
                "torch",
                35,
                "PyTorch is already installed.",
            ));
        } else {
            if has_torch {
                progress(DependencyProgress::new(
                    "remove-torch",
                    25,
                    "Removing the previous PyTorch build...",
                ));
                uninstall_torch(&python_path, log, cancellation)?;
            }
            progress(DependencyProgress::new(
                "torch",
                40,
                "Installing PyTorch...",
            ));
            install_torch(&python_path, target_device, log, cancellation)?;
            progress(DependencyProgress::new(
                "torch",
                60,
                "PyTorch installation finished.",
            ));
        }

        if !has_spec(&python_path, "whisper", cancellation) {
            progress(DependencyProgress::new(
                "whisper",
                70,
                "Installing openai-whisper...",
            ));
            run_with_retry(
                &python_path,
                &[
                    "-m".to_string(),
                    "pip".to_string(),
                    "install".to_string(),
                    WHISPER_PACKAGE.to_string(),
                ],
                PACKAGE_INSTALL_TIMEOUT,
                log,
                cancellation,
            )?;
        } else {
            progress(DependencyProgress::new(
                "whisper",
                75,
                "openai-whisper is already installed.",
            ));
        }

        progress(DependencyProgress::new(
            "verify",
            90,
            "Verifying Python modules...",
        ));
        verify_imports(&python_path, cancellation)?;
        log("Python dependencies are ready.".to_string());
        progress(DependencyProgress::new(
            "complete",
            100,
            "Dependencies are ready.",
        ));
        Ok(())
    }

    pub fn start_transcription(
        request: &TranscriptionRequest,
        cancellation: &CancelToken,
    ) -> Result<TranscriptionProcess, AppError> {
        let runner = AppPaths::executable_directory()
            .join("python")
            .join("transcribe.py");
        if !runner.is_file() {
            return Err(AppError::python("Transcription runner is missing."));
        }
        if !Self::local_python_path().is_file() {
            return Err(AppError::python("The local Python runtime is not ready."));
        }
        let mut child = process::TranscriptionProcess::spawn_transcription(
            &Self::local_python_path().to_string_lossy(),
            &runner.to_string_lossy(),
        )?;
        let payload = serde_json::json!({
            "files": request.files,
            "model": request.model,
            "modelFile": request.model_file,
            "cacheDirectory": request.cache_directory,
            "device": request.device,
            "outputDirectory": request.output_directory,
            "language": request.language,
        })
        .to_string();
        child.write_payload(&payload)?;
        if cancellation.is_cancelled() {
            child.terminate();
            return Err(AppError::Cancelled);
        }
        Ok(child)
    }
}

fn silent() -> LogSink {
    Arc::new(|_| {})
}

fn ensure_runtime(log: &LogSink, cancellation: &CancelToken) -> Result<String, AppError> {
    let local = PythonEnvironment::local_python_path();
    if local.is_file() {
        return Ok(local.to_string_lossy().into_owned());
    }
    let (python, _) = find_system_python(cancellation);
    let Some(python) = python else {
        return Err(AppError::python("Python 3 was not found on this system."));
    };
    let directory = PythonEnvironment::runtime_directory();
    AppPaths::ensure_directory(&directory)
        .map_err(|error| AppError::python(AppLog::redact(&error.to_string())))?;
    log("Creating the application-local Python environment...".to_string());
    process::run_logged(
        &python,
        &[
            "-m".to_string(),
            "venv".to_string(),
            directory.to_string_lossy().into_owned(),
        ],
        cancellation.bounded(VENV_TIMEOUT),
        &silent(),
        "Unable to create the local Python environment.",
    )?;
    if !local.is_file() {
        return Err(AppError::python(
            "The local Python environment could not be created.",
        ));
    }
    Ok(local.to_string_lossy().into_owned())
}

fn install_torch(
    python_path: &str,
    target_device: &str,
    log: &LogSink,
    cancellation: &CancelToken,
) -> Result<(), AppError> {
    if target_device == "cpu" {
        return run_with_retry(
            python_path,
            &[
                "-m".to_string(),
                "pip".to_string(),
                "install".to_string(),
                TORCH_PACKAGE.to_string(),
                "--index-url".to_string(),
                "https://download.pytorch.org/whl/cpu".to_string(),
            ],
            TORCH_INSTALL_TIMEOUT,
            log,
            cancellation,
        );
    }

    let driver = hardware::detect_cuda_driver_version(cancellation);
    let variants = hardware::supported_cuda_variants(driver.as_deref());
    if variants.is_empty() {
        return Err(AppError::python("No compatible CUDA build was found."));
    }
    let mut last_error: Option<AppError> = None;
    for variant in variants {
        log(format!("Installing PyTorch {}...", variant.to_uppercase()));
        match run_with_retry(
            python_path,
            &[
                "-m".to_string(),
                "pip".to_string(),
                "install".to_string(),
                TORCH_PACKAGE.to_string(),
                "--index-url".to_string(),
                format!("https://download.pytorch.org/whl/{variant}"),
            ],
            TORCH_INSTALL_TIMEOUT,
            log,
            cancellation,
        ) {
            Ok(()) => {
                if torch_has_cuda(python_path, cancellation) {
                    return Ok(());
                }
                last_error = Some(AppError::python("Installed PyTorch does not expose CUDA."));
            }
            Err(AppError::Cancelled) => return Err(AppError::Cancelled),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| AppError::python("CUDA PyTorch installation failed.")))
}

fn uninstall_torch(
    python_path: &str,
    log: &LogSink,
    cancellation: &CancelToken,
) -> Result<(), AppError> {
    for package in TORCH_PACKAGES {
        log(format!("Removing {package}..."));
        process::run_logged(
            python_path,
            &[
                "-m".to_string(),
                "pip".to_string(),
                "uninstall".to_string(),
                "-y".to_string(),
                package.to_string(),
            ],
            cancellation.bounded(PACKAGE_INSTALL_TIMEOUT),
            log,
            "Unable to remove the previous PyTorch build.",
        )?;
    }
    Ok(())
}

fn run_with_retry(
    python_path: &str,
    arguments: &[String],
    timeout: Duration,
    log: &LogSink,
    cancellation: &CancelToken,
) -> Result<(), AppError> {
    let mut last_error: Option<AppError> = None;
    for attempt in 0..MAXIMUM_INSTALL_ATTEMPTS {
        match process::run_logged(
            python_path,
            arguments,
            cancellation.bounded(timeout),
            log,
            "Unable to start Python.",
        ) {
            Ok(()) => return Ok(()),
            Err(AppError::Cancelled) => return Err(AppError::Cancelled),
            Err(error) => {
                last_error = Some(error);
                if attempt + 1 < MAXIMUM_INSTALL_ATTEMPTS {
                    log(format!("Attempt {} failed. Retrying...", attempt + 1));
                    if !cancellation.sleep(Duration::from_secs(2)) {
                        return Err(AppError::Cancelled);
                    }
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| AppError::python("Python command failed.")))
}

fn find_system_python(cancellation: &CancelToken) -> (Option<String>, String) {
    for candidate in PYTHON_CANDIDATES {
        let Some(resolved) = resolve_command(candidate) else {
            continue;
        };
        let arguments = match candidate == "py.exe" {
            true => vec!["-3".to_string(), "--version".to_string()],
            false => vec!["--version".to_string()],
        };
        let output = process::capture(
            &resolved,
            &arguments,
            cancellation.bounded(PYTHON_PROBE_TIMEOUT),
        );
        if output.success && output.text.contains("Python 3.") {
            return (Some(resolved), output.text.trim().to_string());
        }
    }
    (None, String::new())
}

fn resolve_command(command: &str) -> Option<String> {
    let path = Path::new(command);
    if path.is_absolute() {
        return match path.is_file() {
            true => Some(command.to_string()),
            false => None,
        };
    }
    let entries = std::env::var("PATH").unwrap_or_default();
    for directory in entries.split(';').filter(|value| !value.trim().is_empty()) {
        let candidate = Path::new(directory.trim()).join(command);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

fn probe_specs(python_path: &str, cancellation: &CancelToken) -> (bool, bool) {
    const SCRIPT: &str = "import importlib.util;print(','.join('1' if importlib.util.find_spec(n) else '0' for n in ('whisper','torch')))";
    let output = process::capture(
        python_path,
        &["-c".to_string(), SCRIPT.to_string()],
        cancellation.bounded(PYTHON_PROBE_TIMEOUT),
    );
    if !output.success {
        return (false, false);
    }
    let values = output.text.trim().split(',').collect::<Vec<_>>();
    (
        values.len() == 2 && values[0] == "1",
        values.len() == 2 && values[1] == "1",
    )
}

fn has_spec(python_path: &str, module_name: &str, cancellation: &CancelToken) -> bool {
    let script = format!(
        "import importlib.util;print('1' if importlib.util.find_spec('{module_name}') else '0')"
    );
    let output = process::capture(
        python_path,
        &["-c".to_string(), script],
        cancellation.bounded(PYTHON_PROBE_TIMEOUT),
    );
    output.success && output.text.trim() == "1"
}

fn torch_has_cuda(python_path: &str, cancellation: &CancelToken) -> bool {
    const SCRIPT: &str = "import torch;print('1' if torch.cuda.is_available() else '0')";
    let output = process::capture(
        python_path,
        &["-c".to_string(), SCRIPT.to_string()],
        cancellation.bounded(PYTHON_PROBE_TIMEOUT),
    );
    output.success && output.text.trim() == "1"
}

fn verify_imports(python_path: &str, cancellation: &CancelToken) -> Result<(), AppError> {
    const SCRIPT: &str = "import whisper,torch;print('ok')";
    let output = process::capture(
        python_path,
        &["-c".to_string(), SCRIPT.to_string()],
        cancellation.bounded(IMPORT_PROBE_TIMEOUT),
    );
    if !output.success || !output.text.contains("ok") {
        return Err(AppError::python(
            "Installed modules failed the import check.",
        ));
    }
    Ok(())
}
