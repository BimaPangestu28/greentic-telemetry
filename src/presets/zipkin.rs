use anyhow::Result;

use super::{PresetConfig, parse_headers_from_env};
use crate::export::ExportMode;

/// Zipkin OTLP preset.
///
/// Modern Zipkin accepts OTLP over HTTP. Default endpoint: `http://localhost:9411`.
/// Maps to `OtlpHttp` export mode without special auth headers.
pub fn config() -> Result<PresetConfig> {
    let endpoint = std::env::var("OTLP_ENDPOINT")
        .ok()
        .filter(|ep| !ep.is_empty())
        .or_else(|| Some(String::from("http://localhost:9411")));

    let headers = parse_headers_from_env(std::env::var("OTLP_HEADERS").ok())?;

    Ok(PresetConfig {
        export_mode: Some(ExportMode::OtlpHttp),
        otlp_endpoint: endpoint,
        otlp_headers: headers,
        sampling_ratio: None,
    })
}
