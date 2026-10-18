use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::core::error::AppError;
use crate::core::input;

const BLOCKED_HOST_SUFFIXES: [&str; 8] = [
    ".localhost",
    ".local",
    ".internal",
    ".intranet",
    ".private",
    ".corp",
    ".home",
    ".lan",
];

#[derive(Debug, Clone)]
pub struct HttpsAddress {
    host: String,
    path: String,
    canonical: String,
}

impl HttpsAddress {
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn canonical(&self) -> &str {
        &self.canonical
    }

    pub fn directory_segments(&self) -> Vec<String> {
        self.path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_string())
            .collect()
    }

    pub fn parent_path(&self) -> String {
        let mut segments = self.directory_segments();
        if !segments.is_empty() {
            segments.pop();
        }
        let mut value = String::from("/");
        value.push_str(&segments.join("/"));
        value
    }

    pub fn resolve(&self, location: &str) -> Result<HttpsAddress, AppError> {
        let trimmed = location.trim();
        if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
            return parse_https(trimmed);
        }
        if trimmed.starts_with("//") {
            return parse_https(&format!("https:{trimmed}"));
        }
        if trimmed.starts_with('/') {
            return parse_https(&format!("https://{}{}", self.host, trimmed));
        }
        let base = format!("https://{}{}", self.host, self.parent_path());
        parse_https(&format!("{}/{trimmed}", base.trim_end_matches('/')))
    }
}

pub fn validate_https(value: &str) -> Result<HttpsAddress, AppError> {
    if value.trim().is_empty()
        || value.chars().count() > input::MAX_ADDRESS_LENGTH
        || value.trim().chars().any(char::is_whitespace)
    {
        return Err(invalid_address());
    }
    parse_https(value.trim())
}

pub fn validate_host(host: &str) -> Result<(), AppError> {
    let normalized = host.trim().trim_end_matches('.').to_lowercase();
    if normalized.is_empty()
        || normalized.contains("metadata")
        || normalized.contains("instance-data")
        || BLOCKED_HOST_SUFFIXES
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
    {
        return Err(AppError::network(
            "The host is blocked by the local network policy.",
        ));
    }

    if let Ok(literal) = normalized.parse::<IpAddr>() {
        if is_blocked_address(literal) {
            return Err(AppError::network(
                "The address belongs to a blocked network range.",
            ));
        }
        return Ok(());
    }

    let labels = normalized.split('.').collect::<Vec<_>>();
    if labels.len() < 2
        || labels.iter().any(|label| {
            label.is_empty()
                || label.len() > 63
                || !label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
                || label.starts_with('-')
                || label.ends_with('-')
        })
    {
        return Err(AppError::network("The host name is invalid."));
    }
    Ok(())
}

pub fn filter(addresses: Vec<IpAddr>) -> Result<Vec<IpAddr>, AppError> {
    let allowed = addresses
        .into_iter()
        .filter(|address| !is_blocked_address(*address))
        .collect::<Vec<_>>();
    if allowed.is_empty() {
        return Err(AppError::network("DNS returned only blocked addresses."));
    }
    Ok(allowed)
}

pub fn is_blocked_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(value) => is_blocked_v4(value),
        IpAddr::V6(value) => match value.to_ipv4_mapped() {
            Some(mapped) => is_blocked_v4(mapped),
            None => is_blocked_v6(value),
        },
    }
}

fn is_blocked_v4(address: Ipv4Addr) -> bool {
    let bytes = address.octets();
    bytes[0] == 0
        || bytes[0] == 10
        || bytes[0] == 127
        || (bytes[0] == 169 && bytes[1] == 254)
        || (bytes[0] == 172 && (16..=31).contains(&bytes[1]))
        || (bytes[0] == 192 && bytes[1] == 168)
        || (bytes[0] == 192 && bytes[1] == 0 && bytes[2] == 0)
        || (bytes[0] == 100 && (64..=127).contains(&bytes[1]))
        || (bytes[0] == 198 && (bytes[1] == 18 || bytes[1] == 19))
        || (bytes[0] == 198 && bytes[1] == 51 && bytes[2] == 100)
        || (bytes[0] == 203 && bytes[1] == 0 && bytes[2] == 113)
        || bytes[0] >= 224
}

fn is_blocked_v6(address: Ipv6Addr) -> bool {
    let bytes = address.octets();
    let unique_local = (bytes[0] & 0xFE) == 0xFC;
    let link_local = bytes[0] == 0xFE && (bytes[1] & 0xC0) == 0x80;
    let multicast = bytes[0] == 0xFF;
    let documentation =
        bytes[0] == 0x20 && bytes[1] == 0x01 && bytes[2] == 0x0D && bytes[3] == 0xB8;
    let loopback = bytes[..15].iter().all(|value| *value == 0) && bytes[15] == 1;
    let unspecified = bytes.iter().all(|value| *value == 0);
    unique_local || link_local || multicast || documentation || loopback || unspecified
}

fn invalid_address() -> AppError {
    AppError::network("Only a valid HTTPS address on port 443 is allowed.")
}

fn parse_https(value: &str) -> Result<HttpsAddress, AppError> {
    let invalid = invalid_address;
    let remainder = value.strip_prefix("https://").ok_or_else(invalid)?;
    let (authority, path) = match remainder.find(['/', '?', '#']) {
        Some(index) => (&remainder[..index], &remainder[index..]),
        None => (remainder, ""),
    };
    if authority.is_empty() || authority.contains('@') {
        return Err(invalid());
    }
    let (host, port) = split_authority(authority)?;
    if port != 443 {
        return Err(invalid());
    }
    let host = host.to_lowercase();
    validate_host(&host)?;
    let path = normalize_path(match path.is_empty() {
        true => "/",
        false => path,
    });
    let canonical = format!("https://{host}{path}");
    Ok(HttpsAddress {
        host,
        path,
        canonical,
    })
}

fn normalize_path(path: &str) -> String {
    let (head, tail) = match path.find(['?', '#']) {
        Some(index) => (&path[..index], &path[index..]),
        None => (path, ""),
    };
    let trailing_slash = head.ends_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in head.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            value => segments.push(value),
        }
    }
    let mut value = String::from("/");
    value.push_str(&segments.join("/"));
    if trailing_slash && !segments.is_empty() {
        value.push('/');
    }
    value.push_str(tail);
    value
}

fn split_authority(authority: &str) -> Result<(String, u16), AppError> {
    let invalid = invalid_address;
    let normalized = authority.trim();
    if normalized.starts_with('[') {
        let end = normalized.find(']').ok_or_else(invalid)?;
        let host = normalized[1..end].to_string();
        let rest = &normalized[end + 1..];
        let port = match rest.strip_prefix(':') {
            Some(value) => value.parse::<u16>().map_err(|_| invalid())?,
            None if rest.is_empty() => 443,
            None => return Err(invalid()),
        };
        return Ok((host, port));
    }
    let port = match normalized.matches(':').count() {
        0 => 443,
        1 => {
            let index = normalized.find(':').ok_or_else(invalid)?;
            normalized[index + 1..]
                .parse::<u16>()
                .map_err(|_| invalid())?
        }
        _ => return Err(invalid()),
    };
    let host = match normalized.find(':') {
        Some(index) => normalized[..index].to_string(),
        None => normalized.to_string(),
    };
    if host.is_empty() {
        return Err(invalid());
    }
    Ok((host, port))
}
