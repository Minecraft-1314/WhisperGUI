use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::core::app_log::AppLog;
use crate::core::app_paths::AppPaths;
use crate::core::cancel::CancelToken;
use crate::core::error::AppError;
use crate::core::model_catalog::ModelDefinition;
use crate::core::models::DownloadProgress;
use crate::core::network;
use crate::core::Progress;

const MAXIMUM_ATTEMPTS: usize = 3;
const MAXIMUM_REDIRECTS: usize = 3;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const IDLE_READ_TIMEOUT: Duration = Duration::from_secs(60);
const RETRY_BASE_DELAY: Duration = Duration::from_secs(1);
const BUFFER_SIZE: usize = 256 * 1024;

const RETRY_STATUS: [u16; 6] = [408, 429, 500, 502, 503, 504];

pub fn download(
    model: &ModelDefinition,
    progress: Option<&Progress<DownloadProgress>>,
    cancellation: &CancelToken,
) -> Result<String, AppError> {
    let directory = AppPaths::ensure_directory(&AppPaths::model_cache_directory())
        .map_err(|error| AppError::download(AppLog::redact(&error.to_string())))?;
    let destination = directory.join(&model.file_name);
    if has_expected_size(&destination, model.size_bytes) {
        report(progress, model.size_bytes, model.size_bytes);
        return Ok(destination.to_string_lossy().into_owned());
    }

    let mut last_error = String::new();
    for attempt in 0..MAXIMUM_ATTEMPTS {
        match attempt_download(model, &destination, progress, cancellation) {
            Ok(value) => return Ok(value),
            Err(AppError::Cancelled) => return Err(AppError::Cancelled),
            Err(error) => {
                last_error = error.message();
                if attempt + 1 < MAXIMUM_ATTEMPTS {
                    let delay = RETRY_BASE_DELAY * (1u32 << attempt);
                    if !cancellation.sleep(delay) {
                        return Err(AppError::Cancelled);
                    }
                }
            }
        }
    }
    Err(AppError::download(format!(
        "Model download failed after all retries. {last_error}"
    )))
}

fn attempt_download(
    model: &ModelDefinition,
    destination: &Path,
    progress: Option<&Progress<DownloadProgress>>,
    cancellation: &CancelToken,
) -> Result<String, AppError> {
    let deadline = cancellation.bounded(ATTEMPT_TIMEOUT);
    let activity = Arc::new(AtomicI64::new(0));
    spawn_idle_watchdog(Arc::clone(&activity), deadline.clone());
    let partial = partial_path(destination);
    let agent = build_agent();
    let mut current = model.url.canonical().to_string();

    for _ in 0..=MAXIMUM_REDIRECTS {
        if cancellation.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        let mut existing = safe_length(&partial);
        if existing > model.size_bytes {
            let _ = std::fs::remove_file(&partial);
            existing = 0;
        }

        let mut request = agent.get(&current);
        if existing > 0 {
            request = request.set("Range", &format!("bytes={existing}-"));
        }
        let response = match request.call() {
            Ok(value) => value,
            Err(ureq::Error::Status(code, response)) => {
                if is_redirect(code) {
                    let location = response
                        .header("location")
                        .map(str::to_string)
                        .ok_or_else(|| AppError::download("Redirect without a location."))?;
                    current = model.url.resolve(&location)?.canonical().to_string();
                    continue;
                }
                if RETRY_STATUS.contains(&code) {
                    return Err(AppError::download(format!(
                        "Server returned status {code}."
                    )));
                }
                return Err(AppError::download(format!(
                    "Server returned status {code}."
                )));
            }
            Err(ureq::Error::Transport(error)) => {
                return Err(AppError::download(AppLog::redact(&error.to_string())));
            }
        };

        let status = response.status();
        if is_redirect(status) {
            let location = response
                .header("location")
                .map(str::to_string)
                .ok_or_else(|| AppError::download("Redirect without a location."))?;
            current = model.url.resolve(&location)?.canonical().to_string();
            continue;
        }
        if status != 200 && status != 206 {
            return Err(AppError::download(format!(
                "Server returned status {status}."
            )));
        }

        let resumed = status == 206 && existing > 0;
        let mut received = match resumed {
            true => existing,
            false => 0,
        };
        let content_length = response
            .header("content-length")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(-1);
        if content_length >= 0 && received + content_length != model.size_bytes {
            return Err(AppError::download(
                "Download length does not match the catalog.",
            ));
        }

        let mut target = match resumed {
            true => OpenOptions::new().append(true).open(&partial),
            false => File::create(&partial),
        }
        .map_err(|error| AppError::download(AppLog::redact(&error.to_string())))?;

        let mut source = response.into_reader();
        let mut buffer = vec![0u8; BUFFER_SIZE];
        let mut last_percent = -1i32;
        loop {
            if deadline.is_cancelled() {
                drop(target);
                let _ = std::fs::remove_file(&partial);
                return Err(AppError::download("Model download attempt timed out."));
            }
            let read = source
                .read(&mut buffer)
                .map_err(|error| AppError::download(AppLog::redact(&error.to_string())))?;
            if read == 0 {
                break;
            }
            target
                .write_all(&buffer[..read])
                .map_err(|error| AppError::download(AppLog::redact(&error.to_string())))?;
            received += read as i64;
            activity.store(received, Ordering::SeqCst);
            let percent = ((received.saturating_mul(100) / model.size_bytes).min(100)) as i32;
            if percent != last_percent {
                last_percent = percent;
                report(progress, received, model.size_bytes);
            }
        }
        target
            .flush()
            .map_err(|error| AppError::download(AppLog::redact(&error.to_string())))?;
        drop(target);

        if safe_length(&partial) != model.size_bytes {
            return Err(AppError::download("Downloaded model size is incomplete."));
        }
        let actual = sha256_of(&partial)?;
        if !actual.eq_ignore_ascii_case(&model.sha256) {
            let _ = std::fs::remove_file(&partial);
            return Err(AppError::download("Downloaded model checksum mismatch."));
        }
        std::fs::rename(&partial, destination)
            .map_err(|error| AppError::download(AppLog::redact(&error.to_string())))?;
        report(progress, model.size_bytes, model.size_bytes);
        return Ok(destination.to_string_lossy().into_owned());
    }
    Err(AppError::download("Too many redirects."))
}

fn spawn_idle_watchdog(activity: Arc<AtomicI64>, deadline: CancelToken) {
    std::thread::spawn(move || {
        let mut previous = -1i64;
        let mut stalled = Duration::ZERO;
        while !deadline.is_cancelled() {
            std::thread::sleep(Duration::from_secs(1));
            let current = activity.load(Ordering::SeqCst);
            if current == previous {
                stalled += Duration::from_secs(1);
                if stalled >= IDLE_READ_TIMEOUT {
                    deadline.cancel();
                    return;
                }
            } else {
                previous = current;
                stalled = Duration::ZERO;
            }
        }
    });
}

fn build_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(ATTEMPT_TIMEOUT)
        .timeout_connect(CONNECT_TIMEOUT)
        .resolver(resolve_public)
        .build()
}

fn resolve_public(netloc: &str) -> std::io::Result<Vec<SocketAddr>> {
    let host = match netloc.rsplit_once(':') {
        Some((host, port)) if port.parse::<u16>().is_ok() => host,
        _ => netloc,
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    network::validate_host(host).map_err(blocked)?;
    let addresses = match host.parse::<std::net::IpAddr>() {
        Ok(literal) => vec![literal],
        Err(_) => (host, 0u16)
            .to_socket_addrs()?
            .map(|value| value.ip())
            .collect(),
    };
    let allowed = network::filter(addresses).map_err(blocked)?;
    Ok(allowed
        .into_iter()
        .map(|address| SocketAddr::new(address, 443))
        .collect())
}

fn blocked(error: AppError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::PermissionDenied, error.message())
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

fn report(progress: Option<&Progress<DownloadProgress>>, received: i64, total: i64) {
    if let Some(sink) = progress {
        let percent = ((received.saturating_mul(100) / total.max(1)).min(100)) as i32;
        sink(DownloadProgress {
            received_bytes: received,
            total_bytes: total,
            percent,
        });
    }
}

fn partial_path(destination: &Path) -> std::path::PathBuf {
    let mut value = destination.as_os_str().to_os_string();
    value.push(".part");
    std::path::PathBuf::from(value)
}

fn safe_length(path: &Path) -> i64 {
    std::fs::metadata(path)
        .map(|value| value.len() as i64)
        .unwrap_or(0)
}

fn has_expected_size(path: &Path, expected: i64) -> bool {
    expected > 0 && safe_length(path) == expected
}

pub fn sha256_of(path: &Path) -> Result<String, AppError> {
    let mut file =
        File::open(path).map_err(|error| AppError::download(AppLog::redact(&error.to_string())))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| AppError::download(AppLog::redact(&error.to_string())))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect())
}
