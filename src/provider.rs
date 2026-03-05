use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::export::{Compression, ExportConfig, ExportMode, Sampling};
use crate::init::{TelemetryConfig, init_telemetry_from_config};
use crate::presets;

/// TLS configuration for mTLS connections to OTLP endpoints.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TlsConfig {
    #[serde(default)]
    pub ca_cert_pem: Option<String>,
    #[serde(default)]
    pub client_cert_pem: Option<String>,
    #[serde(default)]
    pub client_key_pem: Option<String>,
}

/// Controls which tenant/team identifiers appear in spans and metrics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TenantAttribution {
    /// Include tenant_id in span attributes (default: true)
    #[serde(default = "default_true")]
    pub include_tenant: bool,
    /// Include team_id in span attributes (default: true)
    #[serde(default = "default_true")]
    pub include_team: bool,
    /// Include team_id in metric labels (default: false — cardinality concern)
    #[serde(default)]
    pub include_team_in_metrics: bool,
    /// Hash tenant/team IDs before emitting (privacy/cardinality)
    #[serde(default)]
    pub hash_ids: bool,
}

impl Default for TenantAttribution {
    fn default() -> Self {
        Self {
            include_tenant: true,
            include_team: true,
            include_team_in_metrics: false,
            hash_ids: false,
        }
    }
}

/// Configuration returned by a telemetry provider component.
///
/// This is the canonical config model for pack-based telemetry setup.
/// A provider WASM component returns this as JSON; the host (operator)
/// passes it to [`init_from_provider_config`] to configure OTel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TelemetryProviderConfig {
    /// Export mode: "otlp-grpc" | "otlp-http" | "json-stdout" | "none"
    #[serde(default = "default_export_mode")]
    pub export_mode: String,

    /// OTLP endpoint (e.g. "http://localhost:4317")
    #[serde(default)]
    pub endpoint: Option<String>,

    /// Auth/metadata headers (typically from secrets)
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// Sampling ratio: 0.0..=1.0
    #[serde(default = "default_sampling_ratio")]
    pub sampling_ratio: f64,

    /// Optional compression: "gzip" | null
    #[serde(default)]
    pub compression: Option<String>,

    /// Service name (default: "greentic-operator")
    #[serde(default)]
    pub service_name: Option<String>,

    /// Additional OTel resource attributes
    #[serde(default)]
    pub resource_attributes: HashMap<String, String>,

    /// Regex patterns for PII redaction
    #[serde(default)]
    pub redaction_patterns: Vec<String>,

    /// Backend preset name: "honeycomb", "datadog", "newrelic", "zipkin", etc.
    #[serde(default)]
    pub preset: Option<String>,

    /// Enable operation subscription telemetry
    #[serde(default = "default_true")]
    pub enable_operation_subs: bool,

    /// Operation subs mode: "metrics_only" | "traces_only" | "metrics_and_traces"
    #[serde(default)]
    pub operation_subs_mode: Option<String>,

    /// Include denied operations in telemetry
    #[serde(default = "default_true")]
    pub include_denied_ops: bool,

    /// Payload policy: "none" | "hash_only"
    #[serde(default)]
    pub payload_policy: Option<String>,

    /// Minimum log level: "trace" | "debug" | "info" | "warn" | "error"
    /// Applied via RUST_LOG before OTel init (env var takes precedence).
    #[serde(default)]
    pub min_log_level: Option<String>,

    /// TLS configuration for mTLS connections to OTLP endpoints.
    #[serde(default)]
    pub tls_config: Option<TlsConfig>,

    /// Operation names to skip in telemetry emission.
    #[serde(default)]
    pub exclude_ops: Vec<String>,

    /// Hard-drop all payload content from telemetry.
    #[serde(default)]
    pub drop_payloads: bool,

    /// Multi-tenant attribution controls.
    #[serde(default)]
    pub tenant_attribution: Option<TenantAttribution>,
}

fn default_export_mode() -> String {
    "json-stdout".into()
}

fn default_sampling_ratio() -> f64 {
    1.0
}

fn default_true() -> bool {
    true
}

impl Default for TelemetryProviderConfig {
    fn default() -> Self {
        Self {
            export_mode: default_export_mode(),
            endpoint: None,
            headers: HashMap::new(),
            sampling_ratio: 1.0,
            compression: None,
            service_name: None,
            resource_attributes: HashMap::new(),
            redaction_patterns: Vec::new(),
            preset: None,
            enable_operation_subs: true,
            operation_subs_mode: None,
            include_denied_ops: true,
            payload_policy: None,
            min_log_level: None,
            tls_config: None,
            exclude_ops: Vec::new(),
            drop_payloads: false,
            tenant_attribution: None,
        }
    }
}

/// Convert a [`TelemetryProviderConfig`] into an [`ExportConfig`]
/// suitable for [`init_telemetry_from_config`].
pub fn to_export_config(config: &TelemetryProviderConfig) -> ExportConfig {
    let mode = match config.export_mode.to_ascii_lowercase().as_str() {
        "otlp-grpc" => ExportMode::OtlpGrpc,
        "otlp-http" => ExportMode::OtlpHttp,
        "azure-appinsights" => ExportMode::AzureAppInsights,
        "aws-xray" => ExportMode::AwsXRay,
        "gcp-cloud-trace" => ExportMode::GcpCloudTrace,
        "json-stdout" => ExportMode::JsonStdout,
        "none" => ExportMode::JsonStdout,
        _ => ExportMode::JsonStdout,
    };

    let sampling = if config.sampling_ratio <= 0.0 {
        Sampling::AlwaysOff
    } else if config.sampling_ratio >= 1.0 {
        Sampling::AlwaysOn
    } else {
        Sampling::TraceIdRatio(config.sampling_ratio)
    };

    let compression =
        config
            .compression
            .as_deref()
            .and_then(|c| match c.to_ascii_lowercase().as_str() {
                "gzip" => Some(Compression::Gzip),
                _ => None,
            });

    ExportConfig {
        mode,
        endpoint: config.endpoint.clone(),
        headers: config.headers.clone(),
        sampling,
        compression,
        resource_attributes: config.resource_attributes.clone(),
        tls_config: config.tls_config.clone(),
    }
}

/// Resolve a preset name to a base [`ExportConfig`], then overlay
/// any explicit fields from the provider config on top.
fn resolve_with_preset(config: &TelemetryProviderConfig) -> Result<ExportConfig> {
    let preset_name = config.preset.as_deref().unwrap_or("none");
    let preset = match preset_name.to_ascii_lowercase().as_str() {
        "aws" => presets::CloudPreset::Aws,
        "gcp" => presets::CloudPreset::Gcp,
        "azure" => presets::CloudPreset::Azure,
        "datadog" => presets::CloudPreset::Datadog,
        "loki" => presets::CloudPreset::Loki,
        "honeycomb" => presets::CloudPreset::Honeycomb,
        "newrelic" => presets::CloudPreset::NewRelic,
        "elastic" => presets::CloudPreset::Elastic,
        "grafana-tempo" | "grafana_tempo" => presets::CloudPreset::GrafanaTempo,
        "jaeger" => presets::CloudPreset::Jaeger,
        "zipkin" => presets::CloudPreset::Zipkin,
        "otlp-grpc" | "otlp_grpc" => presets::CloudPreset::OtlpGrpc,
        "otlp-http" | "otlp_http" => presets::CloudPreset::OtlpHttp,
        "stdout" => presets::CloudPreset::Stdout,
        _ => presets::CloudPreset::None,
    };

    let preset_cfg = presets::load_preset(preset)?;

    // Start from preset defaults
    let mode = if config.export_mode != "json-stdout" || config.preset.is_none() {
        // Explicit export_mode overrides preset
        match config.export_mode.to_ascii_lowercase().as_str() {
            "otlp-grpc" => ExportMode::OtlpGrpc,
            "otlp-http" => ExportMode::OtlpHttp,
            "azure-appinsights" => ExportMode::AzureAppInsights,
            "aws-xray" => ExportMode::AwsXRay,
            "gcp-cloud-trace" => ExportMode::GcpCloudTrace,
            _ => ExportMode::JsonStdout,
        }
    } else {
        preset_cfg.export_mode.unwrap_or(ExportMode::JsonStdout)
    };

    // Explicit endpoint overrides preset
    let endpoint = config.endpoint.clone().or(preset_cfg.otlp_endpoint);

    // Merge headers: preset defaults + explicit overrides
    let mut headers = preset_cfg.otlp_headers;
    headers.extend(config.headers.clone());

    // Use preset sampling as fallback when user hasn't overridden (default is 1.0)
    let effective_ratio = if (config.sampling_ratio - 1.0).abs() < f64::EPSILON {
        preset_cfg.sampling_ratio.unwrap_or(config.sampling_ratio)
    } else {
        config.sampling_ratio
    };

    let sampling = if effective_ratio <= 0.0 {
        Sampling::AlwaysOff
    } else if effective_ratio >= 1.0 {
        Sampling::AlwaysOn
    } else {
        Sampling::TraceIdRatio(effective_ratio)
    };

    let compression =
        config
            .compression
            .as_deref()
            .and_then(|c| match c.to_ascii_lowercase().as_str() {
                "gzip" => Some(Compression::Gzip),
                _ => None,
            });

    Ok(ExportConfig {
        mode,
        endpoint,
        headers,
        sampling,
        compression,
        resource_attributes: config.resource_attributes.clone(),
        tls_config: config.tls_config.clone(),
    })
}

/// Initialize the full OTel pipeline from a provider config.
///
/// If a `preset` is specified, resolves the preset first, then overlays
/// any explicit fields from the config. Otherwise, converts directly.
///
/// Redaction patterns from the config are applied by setting `PII_MASK_REGEXES`
/// before the telemetry pipeline initializes the redactor.
pub fn init_from_provider_config(config: &TelemetryProviderConfig) -> Result<()> {
    // Set RUST_LOG from min_log_level if not already set by the environment.
    if let Some(ref level) = config.min_log_level
        && std::env::var("RUST_LOG").is_err()
    {
        // Safety: called early in single-threaded init path before spawning workers.
        unsafe {
            std::env::set_var("RUST_LOG", level);
        }
    }

    // Set redaction patterns before init (redactor reads PII_MASK_REGEXES once)
    if !config.redaction_patterns.is_empty() {
        let joined = config.redaction_patterns.join(",");
        // Safety: called early in single-threaded init path before spawning workers.
        unsafe {
            std::env::set_var("PII_MASK_REGEXES", &joined);
        }
    }

    let service_name = config
        .service_name
        .clone()
        .unwrap_or_else(|| "greentic-operator".into());

    let export = if config.preset.is_some() {
        resolve_with_preset(config)?
    } else {
        to_export_config(config)
    };

    init_telemetry_from_config(TelemetryConfig { service_name }, export)
}

// ---------------------------------------------------------------------------
// Config validation (called by operator after receiving provider config)
// ---------------------------------------------------------------------------

/// Known export modes.
const KNOWN_EXPORT_MODES: &[&str] = &[
    "otlp-grpc",
    "otlp-http",
    "json-stdout",
    "azure-appinsights",
    "aws-xray",
    "gcp-cloud-trace",
    "none",
];

/// Header keys that should be secrets-backed rather than plain text.
const SENSITIVE_HEADER_KEYS: &[&str] = &[
    "authorization",
    "api-key",
    "x-api-key",
    "x-honeycomb-team",
    "dd_api_key",
    "dd-api-key",
];

const KNOWN_SUBS_MODES: &[&str] = &["metrics_only", "traces_only", "metrics_and_traces"];
const KNOWN_PAYLOAD_POLICIES: &[&str] = &["none", "hash_only"];
const KNOWN_COMPRESSIONS: &[&str] = &["gzip"];
const KNOWN_LOG_LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error"];

/// Validate a [`TelemetryProviderConfig`] and return a list of warnings.
///
/// Checks:
/// - `export_mode` is a known value
/// - `endpoint` is present when export mode requires it (otlp-grpc, otlp-http)
/// - Headers with sensitive keys are flagged (should be secrets-backed)
pub fn validate_telemetry_config(config: &TelemetryProviderConfig) -> Vec<String> {
    let mut warnings = Vec::new();
    let mode_lower = config.export_mode.to_ascii_lowercase();

    // 1. Unknown export mode
    if !KNOWN_EXPORT_MODES.contains(&mode_lower.as_str()) {
        warnings.push(format!(
            "unknown export_mode '{}'; expected one of: {}",
            config.export_mode,
            KNOWN_EXPORT_MODES.join(", ")
        ));
    }

    // 2. Endpoint required for OTLP modes (unless preset provides a default)
    let needs_endpoint = matches!(mode_lower.as_str(), "otlp-grpc" | "otlp-http");
    if needs_endpoint && config.endpoint.is_none() && config.preset.is_none() {
        warnings.push(format!(
            "export_mode '{}' requires an endpoint but none is configured and no preset is set",
            config.export_mode
        ));
    }

    // 3. Sensitive headers should be secrets-backed
    for key in config.headers.keys() {
        if SENSITIVE_HEADER_KEYS.contains(&key.to_ascii_lowercase().as_str()) {
            warnings.push(format!(
                "header '{}' appears to contain credentials; consider using secrets-backed values",
                key
            ));
        }
    }

    // 4. sampling_ratio out of range
    if !(0.0..=1.0).contains(&config.sampling_ratio) {
        warnings.push(format!(
            "sampling_ratio {} is out of range 0.0..=1.0",
            config.sampling_ratio
        ));
    }

    // 5. Unknown compression
    if let Some(ref c) = config.compression
        && !KNOWN_COMPRESSIONS.contains(&c.to_ascii_lowercase().as_str())
    {
        warnings.push(format!(
            "unknown compression '{}'; expected one of: {}",
            c,
            KNOWN_COMPRESSIONS.join(", ")
        ));
    }

    // 6. Unknown operation_subs_mode
    if let Some(ref m) = config.operation_subs_mode
        && !KNOWN_SUBS_MODES.contains(&m.to_ascii_lowercase().as_str())
    {
        warnings.push(format!(
            "unknown operation_subs_mode '{}'; expected one of: {}",
            m,
            KNOWN_SUBS_MODES.join(", ")
        ));
    }

    // 7. Unknown payload_policy
    if let Some(ref p) = config.payload_policy
        && !KNOWN_PAYLOAD_POLICIES.contains(&p.to_ascii_lowercase().as_str())
    {
        warnings.push(format!(
            "unknown payload_policy '{}'; expected one of: {}",
            p,
            KNOWN_PAYLOAD_POLICIES.join(", ")
        ));
    }

    // 8. Empty redaction_patterns entry
    if config
        .redaction_patterns
        .iter()
        .any(|p| p.trim().is_empty())
    {
        warnings.push("redaction_patterns contains an empty entry".into());
    }

    // 9. Unknown min_log_level
    if let Some(ref level) = config.min_log_level
        && !KNOWN_LOG_LEVELS.contains(&level.to_ascii_lowercase().as_str())
    {
        warnings.push(format!(
            "unknown min_log_level '{}'; expected one of: {}",
            level,
            KNOWN_LOG_LEVELS.join(", ")
        ));
    }

    // 10. TLS cert without key (or vice versa)
    if let Some(ref tls) = config.tls_config
        && (tls.client_cert_pem.is_some() != tls.client_key_pem.is_some())
    {
        warnings.push(
            "tls_config has client_cert_pem without client_key_pem (or vice versa); both are required for mTLS".into()
        );
    }

    // 11. hash_ids enabled but both tenant and team excluded (hashing unused)
    if let Some(ref attr) = config.tenant_attribution
        && attr.hash_ids
        && !attr.include_tenant
        && !attr.include_team
    {
        warnings.push(
            "tenant_attribution.hash_ids is enabled but both include_tenant and include_team are false; hashing has no effect".into()
        );
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_produces_json_stdout() {
        let config = TelemetryProviderConfig::default();
        let export = to_export_config(&config);
        assert_eq!(export.mode, ExportMode::JsonStdout);
        assert!(export.endpoint.is_none());
        assert!(export.headers.is_empty());
    }

    #[test]
    fn otlp_grpc_config() {
        let config = TelemetryProviderConfig {
            export_mode: "otlp-grpc".into(),
            endpoint: Some("http://collector:4317".into()),
            headers: {
                let mut h = HashMap::new();
                h.insert("x-api-key".into(), "secret123".into());
                h
            },
            sampling_ratio: 0.5,
            compression: Some("gzip".into()),
            ..Default::default()
        };
        let export = to_export_config(&config);
        assert_eq!(export.mode, ExportMode::OtlpGrpc);
        assert_eq!(export.endpoint.as_deref(), Some("http://collector:4317"));
        assert_eq!(export.headers.get("x-api-key").unwrap(), "secret123");
        assert!(
            matches!(export.sampling, Sampling::TraceIdRatio(r) if (r - 0.5).abs() < f64::EPSILON)
        );
        assert!(matches!(export.compression, Some(Compression::Gzip)));
    }

    #[test]
    fn otlp_http_config() {
        let config = TelemetryProviderConfig {
            export_mode: "otlp-http".into(),
            endpoint: Some("http://collector:4318".into()),
            ..Default::default()
        };
        let export = to_export_config(&config);
        assert_eq!(export.mode, ExportMode::OtlpHttp);
    }

    #[test]
    fn none_mode_falls_back_to_json_stdout() {
        let config = TelemetryProviderConfig {
            export_mode: "none".into(),
            ..Default::default()
        };
        let export = to_export_config(&config);
        assert_eq!(export.mode, ExportMode::JsonStdout);
    }

    #[test]
    fn sampling_boundaries() {
        // 0.0 → AlwaysOff
        let config = TelemetryProviderConfig {
            sampling_ratio: 0.0,
            ..Default::default()
        };
        assert!(matches!(
            to_export_config(&config).sampling,
            Sampling::AlwaysOff
        ));

        // 1.0 → AlwaysOn
        let config = TelemetryProviderConfig {
            sampling_ratio: 1.0,
            ..Default::default()
        };
        assert!(matches!(
            to_export_config(&config).sampling,
            Sampling::AlwaysOn
        ));

        // In between → TraceIdRatio
        let config = TelemetryProviderConfig {
            sampling_ratio: 0.25,
            ..Default::default()
        };
        assert!(matches!(
            to_export_config(&config).sampling,
            Sampling::TraceIdRatio(_)
        ));
    }

    #[test]
    fn preset_resolution_honeycomb() {
        let config = TelemetryProviderConfig {
            preset: Some("honeycomb".into()),
            headers: {
                let mut h = HashMap::new();
                h.insert("x-honeycomb-team".into(), "my-key".into());
                h
            },
            ..Default::default()
        };
        let export = resolve_with_preset(&config).unwrap();
        assert_eq!(export.mode, ExportMode::OtlpGrpc);
        assert!(export.endpoint.is_some());
        assert!(export.headers.contains_key("x-honeycomb-team"));
    }

    #[test]
    fn preset_resolution_jaeger() {
        let config = TelemetryProviderConfig {
            preset: Some("jaeger".into()),
            ..Default::default()
        };
        let export = resolve_with_preset(&config).unwrap();
        assert_eq!(export.mode, ExportMode::OtlpGrpc);
        assert_eq!(export.endpoint.as_deref(), Some("http://localhost:4317"));
    }

    #[test]
    fn explicit_endpoint_overrides_preset() {
        let config = TelemetryProviderConfig {
            preset: Some("honeycomb".into()),
            endpoint: Some("http://custom:4317".into()),
            ..Default::default()
        };
        let export = resolve_with_preset(&config).unwrap();
        assert_eq!(export.endpoint.as_deref(), Some("http://custom:4317"));
    }

    #[test]
    fn compression_gzip_parsed() {
        let config = TelemetryProviderConfig {
            compression: Some("gzip".into()),
            ..Default::default()
        };
        let export = to_export_config(&config);
        assert!(matches!(export.compression, Some(Compression::Gzip)));
    }

    #[test]
    fn compression_unknown_ignored() {
        let config = TelemetryProviderConfig {
            compression: Some("lz4".into()),
            ..Default::default()
        };
        let export = to_export_config(&config);
        assert!(export.compression.is_none());
    }

    #[test]
    fn resource_attributes_passed_through() {
        let config = TelemetryProviderConfig {
            resource_attributes: {
                let mut m = HashMap::new();
                m.insert("deployment.environment".into(), "staging".into());
                m.insert("service.version".into(), "1.2.3".into());
                m
            },
            ..Default::default()
        };
        let export = to_export_config(&config);
        assert_eq!(
            export
                .resource_attributes
                .get("deployment.environment")
                .unwrap(),
            "staging"
        );
        assert_eq!(
            export.resource_attributes.get("service.version").unwrap(),
            "1.2.3"
        );
    }

    #[test]
    fn resource_attributes_passed_through_preset() {
        let config = TelemetryProviderConfig {
            preset: Some("jaeger".into()),
            resource_attributes: {
                let mut m = HashMap::new();
                m.insert("k8s.pod.name".into(), "test-pod".into());
                m
            },
            ..Default::default()
        };
        let export = resolve_with_preset(&config).unwrap();
        assert_eq!(
            export.resource_attributes.get("k8s.pod.name").unwrap(),
            "test-pod"
        );
    }

    #[test]
    fn default_service_name_is_greentic_operator() {
        // init_from_provider_config uses "greentic-operator" when service_name is None.
        // We can't easily test the full init (it's idempotent + global), so verify
        // the default value is correct in the config.
        let config = TelemetryProviderConfig::default();
        assert!(config.service_name.is_none());
        let name = config
            .service_name
            .unwrap_or_else(|| "greentic-operator".into());
        assert_eq!(name, "greentic-operator");
    }

    #[test]
    fn custom_service_name_used() {
        let config = TelemetryProviderConfig {
            service_name: Some("my-service".into()),
            ..Default::default()
        };
        assert_eq!(config.service_name.as_deref(), Some("my-service"));
    }

    // --- validate_telemetry_config tests ---

    #[test]
    fn validate_default_config_no_warnings() {
        let config = TelemetryProviderConfig::default();
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_unknown_export_mode() {
        let config = TelemetryProviderConfig {
            export_mode: "kafka".into(),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unknown export_mode"));
    }

    #[test]
    fn validate_otlp_grpc_without_endpoint_warns() {
        let config = TelemetryProviderConfig {
            export_mode: "otlp-grpc".into(),
            endpoint: None,
            preset: None,
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.iter().any(|w| w.contains("requires an endpoint")));
    }

    #[test]
    fn validate_otlp_grpc_with_preset_no_endpoint_ok() {
        let config = TelemetryProviderConfig {
            export_mode: "otlp-grpc".into(),
            endpoint: None,
            preset: Some("jaeger".into()),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(!warnings.iter().any(|w| w.contains("requires an endpoint")));
    }

    #[test]
    fn validate_otlp_grpc_with_endpoint_ok() {
        let config = TelemetryProviderConfig {
            export_mode: "otlp-grpc".into(),
            endpoint: Some("http://localhost:4317".into()),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_sensitive_header_warns() {
        let config = TelemetryProviderConfig {
            headers: {
                let mut h = HashMap::new();
                h.insert("x-honeycomb-team".into(), "my-key".into());
                h
            },
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("credentials"));
    }

    #[test]
    fn validate_non_sensitive_header_ok() {
        let config = TelemetryProviderConfig {
            headers: {
                let mut h = HashMap::new();
                h.insert("x-custom-header".into(), "value".into());
                h
            },
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_sampling_ratio_out_of_range() {
        let config = TelemetryProviderConfig {
            sampling_ratio: -0.5,
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.iter().any(|w| w.contains("sampling_ratio")));

        let config = TelemetryProviderConfig {
            sampling_ratio: 1.5,
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.iter().any(|w| w.contains("sampling_ratio")));
    }

    #[test]
    fn validate_unknown_compression() {
        let config = TelemetryProviderConfig {
            compression: Some("lz4".into()),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.iter().any(|w| w.contains("unknown compression")));
    }

    #[test]
    fn validate_unknown_operation_subs_mode() {
        let config = TelemetryProviderConfig {
            operation_subs_mode: Some("everything".into()),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("unknown operation_subs_mode"))
        );
    }

    #[test]
    fn validate_unknown_payload_policy() {
        let config = TelemetryProviderConfig {
            payload_policy: Some("full_body".into()),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("unknown payload_policy"))
        );
    }

    #[test]
    fn validate_empty_redaction_pattern() {
        let config = TelemetryProviderConfig {
            redaction_patterns: vec!["\\d+".into(), "".into()],
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.iter().any(|w| w.contains("empty entry")));
    }

    #[test]
    fn validate_unknown_min_log_level() {
        let config = TelemetryProviderConfig {
            min_log_level: Some("verbose".into()),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.iter().any(|w| w.contains("unknown min_log_level")));
    }

    #[test]
    fn validate_valid_min_log_level_ok() {
        let config = TelemetryProviderConfig {
            min_log_level: Some("debug".into()),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_tls_cert_without_key() {
        let config = TelemetryProviderConfig {
            tls_config: Some(TlsConfig {
                ca_cert_pem: None,
                client_cert_pem: Some("cert-data".into()),
                client_key_pem: None,
            }),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("client_cert_pem without client_key_pem"))
        );
    }

    #[test]
    fn validate_tls_complete_ok() {
        let config = TelemetryProviderConfig {
            tls_config: Some(TlsConfig {
                ca_cert_pem: Some("ca".into()),
                client_cert_pem: Some("cert".into()),
                client_key_pem: Some("key".into()),
            }),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn preset_resolution_zipkin() {
        let config = TelemetryProviderConfig {
            preset: Some("zipkin".into()),
            ..Default::default()
        };
        let export = resolve_with_preset(&config).unwrap();
        assert_eq!(export.mode, ExportMode::OtlpHttp);
        assert_eq!(export.endpoint.as_deref(), Some("http://localhost:9411"));
    }

    #[test]
    fn new_fields_default_values() {
        let config = TelemetryProviderConfig::default();
        assert!(config.min_log_level.is_none());
        assert!(config.tls_config.is_none());
        assert!(config.exclude_ops.is_empty());
        assert!(!config.drop_payloads);
        assert!(config.tenant_attribution.is_none());
    }

    #[test]
    fn new_fields_serde_roundtrip() {
        let config = TelemetryProviderConfig {
            min_log_level: Some("debug".into()),
            tls_config: Some(TlsConfig {
                ca_cert_pem: Some("ca-data".into()),
                client_cert_pem: None,
                client_key_pem: None,
            }),
            exclude_ops: vec!["healthcheck".into(), "ping".into()],
            drop_payloads: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TelemetryProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.min_log_level.as_deref(), Some("debug"));
        assert!(deserialized.tls_config.is_some());
        assert_eq!(deserialized.exclude_ops.len(), 2);
        assert!(deserialized.drop_payloads);
    }

    // --- Story 4.1: Convenience presets ---

    #[test]
    fn preset_resolution_otlp_grpc() {
        let config = TelemetryProviderConfig {
            preset: Some("otlp-grpc".into()),
            ..Default::default()
        };
        let export = resolve_with_preset(&config).unwrap();
        assert_eq!(export.mode, ExportMode::OtlpGrpc);
        assert_eq!(export.endpoint.as_deref(), Some("http://localhost:4317"));
    }

    #[test]
    fn preset_resolution_otlp_http() {
        let config = TelemetryProviderConfig {
            preset: Some("otlp-http".into()),
            ..Default::default()
        };
        let export = resolve_with_preset(&config).unwrap();
        assert_eq!(export.mode, ExportMode::OtlpHttp);
        assert_eq!(export.endpoint.as_deref(), Some("http://localhost:4318"));
    }

    #[test]
    fn preset_resolution_stdout() {
        let config = TelemetryProviderConfig {
            preset: Some("stdout".into()),
            ..Default::default()
        };
        let export = resolve_with_preset(&config).unwrap();
        assert_eq!(export.mode, ExportMode::JsonStdout);
        assert!(export.endpoint.is_none());
    }

    #[test]
    fn preset_sampling_fallback() {
        // When user hasn't changed sampling (default 1.0) and preset provides a ratio,
        // the preset's ratio should be used.
        // Current presets all return None, so the fallback should remain 1.0 (AlwaysOn).
        let config = TelemetryProviderConfig {
            preset: Some("jaeger".into()),
            ..Default::default()
        };
        let export = resolve_with_preset(&config).unwrap();
        assert!(matches!(export.sampling, Sampling::AlwaysOn));
    }

    #[test]
    fn explicit_sampling_overrides_preset() {
        let config = TelemetryProviderConfig {
            preset: Some("jaeger".into()),
            sampling_ratio: 0.5,
            ..Default::default()
        };
        let export = resolve_with_preset(&config).unwrap();
        assert!(
            matches!(export.sampling, Sampling::TraceIdRatio(r) if (r - 0.5).abs() < f64::EPSILON)
        );
    }

    // --- Story 4.2: Tenant attribution ---

    #[test]
    fn tenant_attribution_default_values() {
        let attr = TenantAttribution::default();
        assert!(attr.include_tenant);
        assert!(attr.include_team);
        assert!(!attr.include_team_in_metrics);
        assert!(!attr.hash_ids);
    }

    #[test]
    fn tenant_attribution_serde_roundtrip() {
        let config = TelemetryProviderConfig {
            tenant_attribution: Some(TenantAttribution {
                include_tenant: true,
                include_team: false,
                include_team_in_metrics: true,
                hash_ids: true,
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TelemetryProviderConfig = serde_json::from_str(&json).unwrap();
        let attr = deserialized.tenant_attribution.unwrap();
        assert!(attr.include_tenant);
        assert!(!attr.include_team);
        assert!(attr.include_team_in_metrics);
        assert!(attr.hash_ids);
    }

    #[test]
    fn validate_hash_ids_without_includes_warns() {
        let config = TelemetryProviderConfig {
            tenant_attribution: Some(TenantAttribution {
                include_tenant: false,
                include_team: false,
                include_team_in_metrics: false,
                hash_ids: true,
            }),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.iter().any(|w| w.contains("hash_ids")));
    }

    #[test]
    fn validate_hash_ids_with_includes_ok() {
        let config = TelemetryProviderConfig {
            tenant_attribution: Some(TenantAttribution {
                include_tenant: true,
                include_team: true,
                include_team_in_metrics: false,
                hash_ids: true,
            }),
            ..Default::default()
        };
        let warnings = validate_telemetry_config(&config);
        assert!(!warnings.iter().any(|w| w.contains("hash_ids")));
    }

    #[test]
    fn tenant_attribution_none_no_warnings() {
        let config = TelemetryProviderConfig::default();
        let warnings = validate_telemetry_config(&config);
        assert!(warnings.is_empty());
    }
}
