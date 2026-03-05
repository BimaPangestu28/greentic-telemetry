use anyhow::Result;

use super::{PresetConfig, parse_headers_from_env};
use crate::export::ExportMode;

/// GCP Cloud Trace preset.
///
/// The real endpoint and auth headers are injected by the telemetry provider
/// WASM component (stage-2). This stage-1 fallback only honours env vars.
pub fn config() -> Result<PresetConfig> {
    let endpoint = std::env::var("OTLP_ENDPOINT")
        .ok()
        .filter(|ep| !ep.is_empty());

    let headers = parse_headers_from_env(std::env::var("OTLP_HEADERS").ok())?;

    Ok(PresetConfig {
        export_mode: Some(ExportMode::GcpCloudTrace),
        otlp_endpoint: endpoint,
        otlp_headers: headers,
        sampling_ratio: None,
    })
}
