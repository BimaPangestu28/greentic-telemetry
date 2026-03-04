use anyhow::Result;

use super::PresetConfig;
use crate::export::ExportMode;

/// Stdout JSON preset.
///
/// Outputs telemetry as JSON to stdout. No endpoint or headers needed.
pub fn config() -> Result<PresetConfig> {
    Ok(PresetConfig {
        export_mode: Some(ExportMode::JsonStdout),
        otlp_endpoint: None,
        otlp_headers: Default::default(),
        sampling_ratio: None,
    })
}
