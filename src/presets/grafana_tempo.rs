use anyhow::Result;

use super::{PresetConfig, parse_headers_from_env};
use crate::export::ExportMode;

/// Grafana Tempo OTLP preset.
///
/// User-provided endpoint, defaults to `http://localhost:4317`.
/// Tempo accepts standard OTLP gRPC without special auth headers.
/// For Grafana Cloud, set auth via `OTLP_HEADERS` env or provider secrets.
pub fn config() -> Result<PresetConfig> {
    let endpoint = std::env::var("OTLP_ENDPOINT")
        .ok()
        .filter(|ep| !ep.is_empty())
        .or_else(|| Some(String::from("http://localhost:4317")));

    let headers = parse_headers_from_env(std::env::var("OTLP_HEADERS").ok())?;

    Ok(PresetConfig {
        export_mode: Some(ExportMode::OtlpGrpc),
        otlp_endpoint: endpoint,
        otlp_headers: headers,
        sampling_ratio: None,
    })
}
