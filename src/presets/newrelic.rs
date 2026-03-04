use anyhow::Result;

use super::{PresetConfig, parse_headers_from_env};
use crate::export::ExportMode;

/// New Relic OTLP preset.
///
/// Default endpoint: `https://otlp.nr-data.net:4317`
/// Auth header: `api-key` (set via `NEW_RELIC_API_KEY` env or provider secrets)
pub fn config() -> Result<PresetConfig> {
    let endpoint = std::env::var("OTLP_ENDPOINT")
        .ok()
        .filter(|ep| !ep.is_empty())
        .or_else(|| Some(String::from("https://otlp.nr-data.net:4317")));

    let mut headers = parse_headers_from_env(std::env::var("OTLP_HEADERS").ok())?;
    if let Some(api_key) = std::env::var("NEW_RELIC_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
    {
        headers.entry("api-key".into()).or_insert(api_key);
    }

    Ok(PresetConfig {
        export_mode: Some(ExportMode::OtlpGrpc),
        otlp_endpoint: endpoint,
        otlp_headers: headers,
        sampling_ratio: None,
    })
}
