use std::net::ToSocketAddrs;
use std::time::Duration;

use crate::core::cancel::CancelToken;
use crate::core::error::AppError;
use crate::core::model_catalog::ModelDefinition;
use crate::core::models::DownloadProgress;
use crate::core::network;
use crate::core::Progress;

const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
const PROBE_BYTES: usize = 256 * 1024;

pub struct ProbeReport {
    pub reachable: bool,
    pub status: u16,
    pub bytes: usize,
    pub checksum_segment_ok: bool,
}

pub fn probe_first_chunk(
    model: &ModelDefinition,
    progress: &Progress<DownloadProgress>,
    cancellation: &CancelToken,
) -> Result<ProbeReport, AppError> {
    let _deadline = cancellation.bounded(PROBE_TIMEOUT);
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(PROBE_TIMEOUT)
        .timeout_connect(Duration::from_secs(15))
        .resolver(resolve_public)
        .build();
    let mut current = model.url.canonical().to_string();
    for _ in 0..4 {
        let response = match agent.get(&current).call() {
            Ok(value) => value,
            Err(ureq::Error::Status(code, response)) if (300..400).contains(&code) => {
                let location = response
                    .header("location")
                    .map(str::to_string)
                    .ok_or_else(|| AppError::download("Redirect without a location."))?;
                current = model.url.resolve(&location)?.canonical().to_string();
                continue;
            }
            Err(ureq::Error::Status(code, _)) => {
                return Ok(ProbeReport {
                    reachable: false,
                    status: code,
                    bytes: 0,
                    checksum_segment_ok: false,
                })
            }
            Err(ureq::Error::Transport(error)) => {
                return Err(AppError::download(crate::core::app_log::AppLog::redact(
                    &error.to_string(),
                )))
            }
        };
        let status = response.status();
        if (300..400).contains(&status) {
            let location = response
                .header("location")
                .map(str::to_string)
                .ok_or_else(|| AppError::download("Redirect without a location."))?;
            current = model.url.resolve(&location)?.canonical().to_string();
            continue;
        }
        let mut source = response.into_reader();
        let mut buffer = vec![0u8; PROBE_BYTES];
        let read = std::io::Read::read(&mut source, &mut buffer).map_err(|error| {
            AppError::download(crate::core::app_log::AppLog::redact(&error.to_string()))
        })?;
        progress(DownloadProgress {
            received_bytes: read as i64,
            total_bytes: model.size_bytes,
            percent: ((read as i64).saturating_mul(100) / model.size_bytes.max(1)) as i32,
        });
        let segments = model.url.directory_segments();
        let checksum_segment_ok = segments
            .get(segments.len().saturating_sub(2))
            .map(|value| value.eq_ignore_ascii_case(&model.sha256))
            .unwrap_or(false);
        return Ok(ProbeReport {
            reachable: read > 0,
            status,
            bytes: read,
            checksum_segment_ok,
        });
    }
    Err(AppError::download("Too many redirects."))
}

fn resolve_public(netloc: &str) -> std::io::Result<Vec<std::net::SocketAddr>> {
    let host = match netloc.rsplit_once(':') {
        Some((host, port)) if port.parse::<u16>().is_ok() => host,
        _ => netloc,
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    network::validate_host(host).map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, error.message())
    })?;
    let addresses = match host.parse::<std::net::IpAddr>() {
        Ok(literal) => vec![literal],
        Err(_) => (host, 0u16)
            .to_socket_addrs()
            .map(|values| values.map(|value| value.ip()).collect())?,
    };
    let allowed = network::filter(addresses).map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, error.message())
    })?;
    Ok(allowed
        .into_iter()
        .map(|address| std::net::SocketAddr::new(address, 443))
        .collect())
}
