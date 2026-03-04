use anyhow::Result;

use super::PresetConfig;
use crate::export::ExportMode;

/// Generic OTLP HTTP preset.
///
/// Default endpoint: `http://localhost:4318` (standard OTLP HTTP port).
/// No auth headers — suitable for local collectors.
pub fn config() -> Result<PresetConfig> {
    Ok(PresetConfig {
        export_mode: Some(ExportMode::OtlpHttp),
        otlp_endpoint: Some("http://localhost:4318".into()),
        otlp_headers: Default::default(),
        sampling_ratio: None,
    })
}
