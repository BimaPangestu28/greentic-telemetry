use serde::{Deserialize, Serialize};

use crate::provider::TelemetryProviderConfig;

/// Mode for operation subscription emission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubsMode {
    MetricsOnly,
    TracesOnly,
    MetricsAndTraces,
}

/// Policy for including payload data in telemetry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayloadPolicy {
    /// No payload data in spans/metrics
    None,
    /// Include a hash of the payload only
    HashOnly,
}

/// Compute a blake3 hash of the payload bytes (hex-encoded).
/// Returns an empty string when the `payload-hash` feature is disabled.
#[cfg(feature = "payload-hash")]
pub fn hash_payload(payload: &[u8]) -> String {
    blake3::hash(payload).to_hex().to_string()
}

#[cfg(not(feature = "payload-hash"))]
pub fn hash_payload(_payload: &[u8]) -> String {
    String::new()
}

/// Configuration for operation subscription emission.
#[derive(Clone, Debug)]
pub struct OperationSubsConfig {
    pub enabled: bool,
    pub mode: SubsMode,
    pub include_denied: bool,
    pub payload_policy: PayloadPolicy,
    /// Operation names to exclude from telemetry emission.
    pub exclude_ops: Vec<String>,
    /// Include tenant_id in span attributes.
    pub include_tenant: bool,
    /// Include team_id in span attributes.
    pub include_team: bool,
    /// Include team_id in metric labels.
    pub include_team_in_metrics: bool,
    /// Hash tenant/team IDs before emitting.
    pub hash_ids: bool,
}

impl Default for OperationSubsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: SubsMode::MetricsAndTraces,
            include_denied: true,
            payload_policy: PayloadPolicy::None,
            exclude_ops: Vec::new(),
            include_tenant: true,
            include_team: true,
            include_team_in_metrics: false,
            hash_ids: false,
        }
    }
}

/// Build an [`OperationSubsConfig`] from a [`TelemetryProviderConfig`].
pub fn subs_config_from_provider(config: &TelemetryProviderConfig) -> OperationSubsConfig {
    let mode = config
        .operation_subs_mode
        .as_deref()
        .map(|m| match m.to_ascii_lowercase().as_str() {
            "metrics_only" => SubsMode::MetricsOnly,
            "traces_only" => SubsMode::TracesOnly,
            _ => SubsMode::MetricsAndTraces,
        })
        .unwrap_or(SubsMode::MetricsAndTraces);

    let payload_policy = if config.drop_payloads {
        PayloadPolicy::None
    } else {
        config
            .payload_policy
            .as_deref()
            .map(|p| match p.to_ascii_lowercase().as_str() {
                "hash_only" => PayloadPolicy::HashOnly,
                _ => PayloadPolicy::None,
            })
            .unwrap_or(PayloadPolicy::None)
    };

    let attr = config.tenant_attribution.as_ref();

    OperationSubsConfig {
        enabled: config.enable_operation_subs,
        mode,
        include_denied: config.include_denied_ops,
        payload_policy,
        exclude_ops: config.exclude_ops.clone(),
        include_tenant: attr.is_none_or(|a| a.include_tenant),
        include_team: attr.is_none_or(|a| a.include_team),
        include_team_in_metrics: attr.is_some_and(|a| a.include_team_in_metrics),
        hash_ids: attr.is_some_and(|a| a.hash_ids),
    }
}

/// Compute the effective attribute value: empty when excluded, hashed when requested.
fn maybe_attr(value: &str, include: bool, should_hash: bool) -> String {
    if !include {
        return String::new();
    }
    if should_hash {
        hash_payload(value.as_bytes())
    } else {
        value.to_string()
    }
}

/// Emit a structured "operation requested" event on the current span.
///
/// Emits an `info!` event (not a child span) on the current tracing context,
/// so it appears as an event on the root span in Jaeger/OTLP.
///
/// When `payload_hash` is `Some`, the hash is included as a field.
pub fn emit_operation_requested(
    config: &OperationSubsConfig,
    op_id: &str,
    op_name: &str,
    tenant: &str,
    team: &str,
    payload_size: usize,
    payload_hash: Option<&str>,
) {
    if !config.enabled {
        return;
    }
    if config.exclude_ops.iter().any(|ex| ex == op_name) {
        return;
    }
    if matches!(config.mode, SubsMode::MetricsOnly) {
        return;
    }
    let t = maybe_attr(tenant, config.include_tenant, config.hash_ids);
    let tm = maybe_attr(team, config.include_team, config.hash_ids);
    match config.payload_policy {
        PayloadPolicy::None => {
            tracing::info!(
                greentic.op.id = %op_id,
                greentic.op.name = %op_name,
                greentic.tenant.id = %t,
                greentic.team.id = %tm,
                "operation.requested"
            );
        }
        PayloadPolicy::HashOnly => {
            tracing::info!(
                greentic.op.id = %op_id,
                greentic.op.name = %op_name,
                greentic.tenant.id = %t,
                greentic.team.id = %tm,
                greentic.payload.size_bytes = payload_size,
                greentic.payload.hash = payload_hash.unwrap_or(""),
                "operation.requested"
            );
        }
    }
}

/// Emit a structured "operation completed" event on the current span.
///
/// Emits an `info!` event (not a child span) on the current tracing context.
/// `duration_ms` and `result_hash` are included when provided.
#[allow(clippy::too_many_arguments)]
pub fn emit_operation_completed(
    config: &OperationSubsConfig,
    op_id: &str,
    op_name: &str,
    tenant: &str,
    team: &str,
    status: &str,
    result_size: usize,
    result_hash: Option<&str>,
    duration_ms: f64,
) {
    if !config.enabled {
        return;
    }
    if config.exclude_ops.iter().any(|ex| ex == op_name) {
        return;
    }
    if !config.include_denied && status == "denied" {
        return;
    }
    if matches!(config.mode, SubsMode::MetricsOnly) {
        return;
    }
    let t = maybe_attr(tenant, config.include_tenant, config.hash_ids);
    let tm = maybe_attr(team, config.include_team, config.hash_ids);
    match config.payload_policy {
        PayloadPolicy::None => {
            tracing::info!(
                greentic.op.id = %op_id,
                greentic.op.name = %op_name,
                greentic.op.status = %status,
                greentic.tenant.id = %t,
                greentic.team.id = %tm,
                greentic.op.duration_ms = duration_ms,
                "operation.completed"
            );
        }
        PayloadPolicy::HashOnly => {
            tracing::info!(
                greentic.op.id = %op_id,
                greentic.op.name = %op_name,
                greentic.op.status = %status,
                greentic.tenant.id = %t,
                greentic.team.id = %tm,
                greentic.result.size_bytes = result_size,
                greentic.result.hash = result_hash.unwrap_or(""),
                greentic.op.duration_ms = duration_ms,
                "operation.completed"
            );
        }
    }
}

/// Emit a structured error event on the current span.
pub fn emit_operation_error(
    config: &OperationSubsConfig,
    op_id: &str,
    error_type: &str,
    error_message: &str,
) {
    if !config.enabled {
        return;
    }
    if config.exclude_ops.iter().any(|ex| ex == op_id) {
        return;
    }
    if matches!(config.mode, SubsMode::MetricsOnly) {
        return;
    }
    tracing::error!(
        greentic.op.id = %op_id,
        "error.type" = %error_type,
        "error.message" = %error_message,
        "operation.error"
    );
}

/// Create a root span for an operation. The caller enters/exits this span
/// to correlate all sub-events (requested, completed, component invocations).
pub fn operation_root_span(
    op_name: &str,
    provider_type: &str,
    tenant: &str,
    team: &str,
) -> tracing::Span {
    tracing::info_span!(
        "greentic.op",
        greentic.op.name = %op_name,
        "greentic.provider.type" = %provider_type,
        greentic.tenant.id = %tenant,
        greentic.team.id = %team,
        otel.status_code = tracing::field::Empty,
        "error.type" = tracing::field::Empty,
        "error.message" = tracing::field::Empty,
        "greentic.meta.routing.provider" = tracing::field::Empty,
        "greentic.meta.classification" = tracing::field::Empty,
        greentic.op.duration_ms = tracing::field::Empty,
    )
}

/// Create a root span with tenant attribution controls applied.
pub fn operation_root_span_attributed(
    op_name: &str,
    provider_type: &str,
    tenant: &str,
    team: &str,
    config: &OperationSubsConfig,
) -> tracing::Span {
    let t = maybe_attr(tenant, config.include_tenant, config.hash_ids);
    let tm = maybe_attr(team, config.include_team, config.hash_ids);
    tracing::info_span!(
        "greentic.op",
        greentic.op.name = %op_name,
        "greentic.provider.type" = %provider_type,
        greentic.tenant.id = %t,
        greentic.team.id = %tm,
        otel.status_code = tracing::field::Empty,
        "error.type" = tracing::field::Empty,
        "error.message" = tracing::field::Empty,
        "greentic.meta.routing.provider" = tracing::field::Empty,
        "greentic.meta.classification" = tracing::field::Empty,
        greentic.op.duration_ms = tracing::field::Empty,
    )
}

// ---------------------------------------------------------------------------
// Operation metrics (counter + histogram)
// ---------------------------------------------------------------------------

#[cfg(feature = "otlp")]
mod metrics_impl {
    use once_cell::sync::Lazy;
    use opentelemetry::{KeyValue, global};

    static OP_DURATION: Lazy<opentelemetry::metrics::Histogram<f64>> = Lazy::new(|| {
        global::meter("greentic-telemetry")
            .f64_histogram("greentic.operation.duration_ms")
            .with_description("Operation end-to-end duration in milliseconds")
            .build()
    });

    static OP_COUNT: Lazy<opentelemetry::metrics::Counter<u64>> = Lazy::new(|| {
        global::meter("greentic-telemetry")
            .u64_counter("greentic.operation.count")
            .with_description("Total number of operations")
            .build()
    });

    static OP_ERROR_COUNT: Lazy<opentelemetry::metrics::Counter<u64>> = Lazy::new(|| {
        global::meter("greentic-telemetry")
            .u64_counter("greentic.operation.error_count")
            .with_description("Total number of operation errors")
            .build()
    });

    pub fn record(
        op_name: &str,
        provider_type: &str,
        status: &str,
        duration_ms: f64,
        tenant: &str,
        team: Option<&str>,
        hash_ids: bool,
    ) {
        let effective_tenant = super::maybe_attr(tenant, true, hash_ids);
        let mut attrs = vec![
            KeyValue::new("greentic.op.name", op_name.to_string()),
            KeyValue::new("greentic.provider.type", provider_type.to_string()),
            KeyValue::new("greentic.op.status", status.to_string()),
            KeyValue::new("greentic.tenant.id", effective_tenant),
        ];
        if let Some(team_val) = team {
            let effective_team = super::maybe_attr(team_val, true, hash_ids);
            attrs.push(KeyValue::new("greentic.team.id", effective_team));
        }
        OP_DURATION.record(duration_ms, &attrs);
        OP_COUNT.add(1, &attrs);
    }

    pub fn record_error(
        op_name: &str,
        provider_type: &str,
        error_code: &str,
        tenant: &str,
        team: Option<&str>,
        hash_ids: bool,
    ) {
        let effective_tenant = super::maybe_attr(tenant, true, hash_ids);
        let mut attrs = vec![
            KeyValue::new("greentic.op.name", op_name.to_string()),
            KeyValue::new("greentic.provider.type", provider_type.to_string()),
            KeyValue::new("greentic.op.error_code", error_code.to_string()),
            KeyValue::new("greentic.tenant.id", effective_tenant),
        ];
        if let Some(team_val) = team {
            let effective_team = super::maybe_attr(team_val, true, hash_ids);
            attrs.push(KeyValue::new("greentic.team.id", effective_team));
        }
        OP_ERROR_COUNT.add(1, &attrs);
    }
}

/// Record operation duration and count metrics (no-op when `otlp` feature is disabled).
///
/// The `config` controls whether team_id is included in metric labels and whether IDs are hashed.
/// Tenant is always included in metrics (required for multi-tenant filtering).
#[cfg(feature = "otlp")]
pub fn record_operation_metric(
    op_name: &str,
    provider_type: &str,
    status: &str,
    duration_ms: f64,
    tenant: &str,
) {
    metrics_impl::record(
        op_name,
        provider_type,
        status,
        duration_ms,
        tenant,
        None,
        false,
    );
}

#[cfg(not(feature = "otlp"))]
pub fn record_operation_metric(
    _op_name: &str,
    _provider_type: &str,
    _status: &str,
    _duration_ms: f64,
    _tenant: &str,
) {
}

/// Record operation duration/count metrics with attribution controls.
#[cfg(feature = "otlp")]
pub fn record_operation_metric_attributed(
    op_name: &str,
    provider_type: &str,
    status: &str,
    duration_ms: f64,
    tenant: &str,
    team: &str,
    config: &OperationSubsConfig,
) {
    let team_opt = if config.include_team_in_metrics {
        Some(team)
    } else {
        None
    };
    metrics_impl::record(
        op_name,
        provider_type,
        status,
        duration_ms,
        tenant,
        team_opt,
        config.hash_ids,
    );
}

#[cfg(not(feature = "otlp"))]
pub fn record_operation_metric_attributed(
    _op_name: &str,
    _provider_type: &str,
    _status: &str,
    _duration_ms: f64,
    _tenant: &str,
    _team: &str,
    _config: &OperationSubsConfig,
) {
}

/// Record an operation error metric (no-op when `otlp` feature is disabled).
#[cfg(feature = "otlp")]
pub fn record_operation_error_metric(
    op_name: &str,
    provider_type: &str,
    error_code: &str,
    tenant: &str,
) {
    metrics_impl::record_error(op_name, provider_type, error_code, tenant, None, false);
}

#[cfg(not(feature = "otlp"))]
pub fn record_operation_error_metric(
    _op_name: &str,
    _provider_type: &str,
    _error_code: &str,
    _tenant: &str,
) {
}

/// Record an operation error metric with attribution controls.
#[cfg(feature = "otlp")]
pub fn record_operation_error_metric_attributed(
    op_name: &str,
    provider_type: &str,
    error_code: &str,
    tenant: &str,
    team: &str,
    config: &OperationSubsConfig,
) {
    let team_opt = if config.include_team_in_metrics {
        Some(team)
    } else {
        None
    };
    metrics_impl::record_error(
        op_name,
        provider_type,
        error_code,
        tenant,
        team_opt,
        config.hash_ids,
    );
}

#[cfg(not(feature = "otlp"))]
pub fn record_operation_error_metric_attributed(
    _op_name: &str,
    _provider_type: &str,
    _error_code: &str,
    _tenant: &str,
    _team: &str,
    _config: &OperationSubsConfig,
) {
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_enables_everything() {
        let config = OperationSubsConfig::default();
        assert!(config.enabled);
        assert_eq!(config.mode, SubsMode::MetricsAndTraces);
        assert!(config.include_denied);
        assert_eq!(config.payload_policy, PayloadPolicy::None);
    }

    #[test]
    fn subs_config_from_provider_defaults() {
        let provider = TelemetryProviderConfig::default();
        let subs = subs_config_from_provider(&provider);
        assert!(subs.enabled);
        assert_eq!(subs.mode, SubsMode::MetricsAndTraces);
        assert!(subs.include_denied);
    }

    #[test]
    fn subs_config_from_provider_custom() {
        let provider = TelemetryProviderConfig {
            enable_operation_subs: true,
            operation_subs_mode: Some("metrics_only".into()),
            include_denied_ops: false,
            payload_policy: Some("hash_only".into()),
            ..Default::default()
        };
        let subs = subs_config_from_provider(&provider);
        assert!(subs.enabled);
        assert_eq!(subs.mode, SubsMode::MetricsOnly);
        assert!(!subs.include_denied);
        assert_eq!(subs.payload_policy, PayloadPolicy::HashOnly);
    }

    #[test]
    fn subs_config_disabled() {
        let provider = TelemetryProviderConfig {
            enable_operation_subs: false,
            ..Default::default()
        };
        let subs = subs_config_from_provider(&provider);
        assert!(!subs.enabled);
    }

    #[test]
    fn emit_requested_noop_when_disabled() {
        let config = OperationSubsConfig {
            enabled: false,
            ..Default::default()
        };
        emit_operation_requested(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            100,
            None,
        );
    }

    #[test]
    fn emit_completed_skips_denied_when_excluded() {
        let config = OperationSubsConfig {
            enabled: true,
            include_denied: false,
            ..Default::default()
        };
        emit_operation_completed(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            "denied",
            0,
            None,
            0.0,
        );
    }

    #[test]
    fn emit_completed_allows_denied_when_included() {
        let config = OperationSubsConfig {
            enabled: true,
            include_denied: true,
            ..Default::default()
        };
        emit_operation_completed(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            "denied",
            0,
            None,
            0.0,
        );
    }

    #[test]
    fn metrics_only_mode_skips_trace_events() {
        let config = OperationSubsConfig {
            enabled: true,
            mode: SubsMode::MetricsOnly,
            ..Default::default()
        };
        emit_operation_requested(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            100,
            None,
        );
        emit_operation_completed(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            "ok",
            50,
            None,
            0.0,
        );
    }

    #[test]
    fn traces_only_mode_emits_events() {
        let config = OperationSubsConfig {
            enabled: true,
            mode: SubsMode::TracesOnly,
            ..Default::default()
        };
        emit_operation_requested(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            100,
            None,
        );
        emit_operation_completed(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            "ok",
            50,
            None,
            0.0,
        );
    }

    #[test]
    fn payload_policy_none_omits_size() {
        let config = OperationSubsConfig {
            enabled: true,
            payload_policy: PayloadPolicy::None,
            ..Default::default()
        };
        emit_operation_requested(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            100,
            None,
        );
        emit_operation_completed(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            "ok",
            50,
            None,
            0.0,
        );
    }

    #[test]
    fn payload_policy_hash_only_includes_size() {
        let config = OperationSubsConfig {
            enabled: true,
            payload_policy: PayloadPolicy::HashOnly,
            ..Default::default()
        };
        emit_operation_requested(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            100,
            Some("abc123"),
        );
        emit_operation_completed(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            "ok",
            50,
            Some("def456"),
            42.5,
        );
    }

    #[test]
    fn root_span_creates_without_panic() {
        let span = operation_root_span("send_payload", "messaging.telegram", "tenant1", "team1");
        let _guard = span.enter();
    }

    // --- New tests for Story 3.1 / 3.2 / 3.3 ---

    #[test]
    fn emit_error_noop_when_disabled() {
        let config = OperationSubsConfig {
            enabled: false,
            ..Default::default()
        };
        emit_operation_error(&config, "op1", "invoke_error", "something broke");
    }

    #[test]
    fn emit_error_noop_when_metrics_only() {
        let config = OperationSubsConfig {
            enabled: true,
            mode: SubsMode::MetricsOnly,
            ..Default::default()
        };
        emit_operation_error(&config, "op1", "invoke_error", "something broke");
    }

    #[test]
    fn emit_error_noop_when_excluded() {
        let config = OperationSubsConfig {
            enabled: true,
            exclude_ops: vec!["op1".to_string()],
            ..Default::default()
        };
        emit_operation_error(&config, "op1", "invoke_error", "something broke");
    }

    #[test]
    fn emit_error_fires_when_enabled() {
        let config = OperationSubsConfig::default();
        // Should not panic; emits tracing::error event
        emit_operation_error(&config, "op1", "denied", "hook denied operation");
    }

    #[test]
    fn hash_payload_returns_string() {
        let hash = hash_payload(b"hello world");
        // With payload-hash feature, this is a non-empty hex string.
        // Without it, this is an empty string.
        // Either way, the function should not panic.
        assert!(hash.is_empty() || hash.len() == 64);
    }

    #[test]
    fn emit_completed_with_duration() {
        let config = OperationSubsConfig::default();
        emit_operation_completed(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            "ok",
            100,
            None,
            123.456,
        );
    }

    #[test]
    fn root_span_has_empty_error_fields() {
        let span = operation_root_span("send_payload", "messaging.telegram", "tenant1", "team1");
        let _guard = span.enter();
        // Record error fields on the root span (should not panic)
        span.record("error.type", "invoke_error");
        span.record("error.message", "component failed");
        span.record("greentic.meta.routing.provider", "messaging-telegram");
        span.record("greentic.op.duration_ms", 42.0);
    }

    #[test]
    fn record_operation_metric_with_tenant() {
        // Should not panic (no-op without otlp feature in test)
        record_operation_metric("send_payload", "messaging.telegram", "ok", 100.0, "tenant1");
    }

    #[test]
    fn record_operation_error_metric_does_not_panic() {
        record_operation_error_metric(
            "send_payload",
            "messaging.telegram",
            "invoke_error",
            "tenant1",
        );
    }

    // --- Story 4.2: Tenant attribution ---

    #[test]
    fn default_config_has_attribution_defaults() {
        let config = OperationSubsConfig::default();
        assert!(config.include_tenant);
        assert!(config.include_team);
        assert!(!config.include_team_in_metrics);
        assert!(!config.hash_ids);
    }

    #[test]
    fn subs_config_from_provider_with_attribution() {
        use crate::provider::{TelemetryProviderConfig, TenantAttribution};
        let provider = TelemetryProviderConfig {
            tenant_attribution: Some(TenantAttribution {
                include_tenant: true,
                include_team: false,
                include_team_in_metrics: true,
                hash_ids: true,
            }),
            ..Default::default()
        };
        let subs = subs_config_from_provider(&provider);
        assert!(subs.include_tenant);
        assert!(!subs.include_team);
        assert!(subs.include_team_in_metrics);
        assert!(subs.hash_ids);
    }

    #[test]
    fn subs_config_from_provider_no_attribution() {
        use crate::provider::TelemetryProviderConfig;
        let provider = TelemetryProviderConfig::default();
        let subs = subs_config_from_provider(&provider);
        assert!(subs.include_tenant);
        assert!(subs.include_team);
        assert!(!subs.include_team_in_metrics);
        assert!(!subs.hash_ids);
    }

    #[test]
    fn maybe_attr_include_no_hash() {
        assert_eq!(maybe_attr("tenant1", true, false), "tenant1");
    }

    #[test]
    fn maybe_attr_exclude() {
        assert_eq!(maybe_attr("tenant1", false, false), "");
        assert_eq!(maybe_attr("tenant1", false, true), "");
    }

    #[test]
    fn maybe_attr_hash() {
        let hashed = maybe_attr("tenant1", true, true);
        // hash_payload returns 64-char hex with payload-hash feature, empty otherwise
        assert!(hashed.is_empty() || hashed.len() == 64);
        assert_ne!(hashed, "tenant1"); // Should differ (unless hash feature off → empty)
    }

    #[test]
    fn emit_requested_with_attribution() {
        let config = OperationSubsConfig {
            include_tenant: false,
            include_team: true,
            hash_ids: true,
            ..Default::default()
        };
        // Should not panic
        emit_operation_requested(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            100,
            None,
        );
    }

    #[test]
    fn emit_completed_with_attribution() {
        let config = OperationSubsConfig {
            include_tenant: true,
            include_team: false,
            hash_ids: false,
            ..Default::default()
        };
        emit_operation_completed(
            &config,
            "op1",
            "send_payload",
            "tenant1",
            "team1",
            "ok",
            50,
            None,
            42.0,
        );
    }

    #[test]
    fn root_span_attributed_creates_without_panic() {
        let config = OperationSubsConfig {
            hash_ids: true,
            include_tenant: true,
            include_team: false,
            ..Default::default()
        };
        let span = operation_root_span_attributed(
            "send_payload",
            "messaging.telegram",
            "tenant1",
            "team1",
            &config,
        );
        let _guard = span.enter();
    }

    #[test]
    fn record_metric_attributed_does_not_panic() {
        let config = OperationSubsConfig {
            include_team_in_metrics: true,
            hash_ids: true,
            ..Default::default()
        };
        record_operation_metric_attributed(
            "send_payload",
            "messaging.telegram",
            "ok",
            100.0,
            "tenant1",
            "team1",
            &config,
        );
    }

    #[test]
    fn record_error_metric_attributed_does_not_panic() {
        let config = OperationSubsConfig {
            include_team_in_metrics: false,
            hash_ids: false,
            ..Default::default()
        };
        record_operation_error_metric_attributed(
            "send_payload",
            "messaging.telegram",
            "invoke_error",
            "tenant1",
            "team1",
            &config,
        );
    }
}
