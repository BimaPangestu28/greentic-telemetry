use anyhow::Result;

use super::PresetConfig;
use crate::export::ExportMode;

/// Generic OTLP gRPC preset.
///
/// Default endpoint: `http://localhost:4317` (standard OTLP gRPC port).
/// No auth headers — suitable for local collectors.
pub fn config() -> Result<PresetConfig> {
    Ok(PresetConfig {
        export_mode: Some(ExportMode::OtlpGrpc),
        otlp_endpoint: Some("http://localhost:4317".into()),
        otlp_headers: Default::default(),
        sampling_ratio: None,
    })
}
