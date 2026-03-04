use anyhow::Result;

use super::{PresetConfig, parse_headers_from_env};
use crate::export::ExportMode;

/// Elastic APM OTLP preset.
///
/// User-provided endpoint (no default — Elastic deployments vary).
/// Auth header: `Authorization` (set via `ELASTIC_APM_SECRET_TOKEN` env or provider secrets)
pub fn config() -> Result<PresetConfig> {
    let endpoint = std::env::var("OTLP_ENDPOINT")
        .ok()
        .filter(|ep| !ep.is_empty());

    let mut headers = parse_headers_from_env(std::env::var("OTLP_HEADERS").ok())?;
    if let Some(token) = std::env::var("ELASTIC_APM_SECRET_TOKEN")
        .ok()
        .filter(|v| !v.is_empty())
    {
        headers
            .entry("Authorization".into())
            .or_insert(format!("Bearer {token}"));
    }

    Ok(PresetConfig {
        export_mode: Some(ExportMode::OtlpGrpc),
        otlp_endpoint: endpoint,
        otlp_headers: headers,
        sampling_ratio: None,
    })
}
