//! State-specific subscription metrics and audit utilities.
//!
//! Provides fine-grained counters for state KV operations (hit/miss, ok/err)
//! and a key-hashing utility to avoid leaking raw keys into audit logs.

use std::fmt::Write;

/// Hash a state key for audit logging purposes.
///
/// Uses a simple FNV-1a 64-bit hash to produce a short hex string.
/// This avoids leaking raw key values into telemetry while still allowing
/// correlation across log entries.
pub fn hash_key_for_audit(key: &str) -> String {
    // FNV-1a 64-bit — fast, no crypto deps needed
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let mut out = String::with_capacity(16);
    let _ = write!(out, "{hash:016x}");
    out
}

/// Emit a state operation pre-subscription event with hashed key.
pub fn emit_state_op_pre(
    config: &crate::OperationSubsConfig,
    op_name: &str,
    namespace: &str,
    key_hash: &str,
    tenant: &str,
    team: &str,
) {
    if !config.enabled {
        return;
    }
    tracing::info_span!("greentic.state.op.pre",
        greentic.op.name = %op_name,
        greentic.state.namespace = %namespace,
        greentic.state.key_hash = %key_hash,
        greentic.tenant.id = %tenant,
        greentic.team.id = %team,
    )
    .in_scope(|| {
        tracing::info!("state.op.requested op={op_name}");
    });
}

/// Emit a state operation post-subscription event with hashed key and outcome.
#[allow(clippy::too_many_arguments)]
pub fn emit_state_op_post(
    config: &crate::OperationSubsConfig,
    op_name: &str,
    namespace: &str,
    key_hash: &str,
    tenant: &str,
    team: &str,
    status: &str,
    duration_ms: f64,
) {
    if !config.enabled {
        return;
    }
    if !config.include_denied && status == "denied" {
        return;
    }
    tracing::info_span!("greentic.state.op.post",
        greentic.op.name = %op_name,
        greentic.state.namespace = %namespace,
        greentic.state.key_hash = %key_hash,
        greentic.tenant.id = %tenant,
        greentic.team.id = %team,
        greentic.op.status = %status,
        greentic.op.duration_ms = %duration_ms,
    )
    .in_scope(|| {
        tracing::info!("state.op.completed op={op_name} status={status}");
    });
}

// ---------------------------------------------------------------------------
// State-specific OTel metrics
// ---------------------------------------------------------------------------

#[cfg(feature = "otlp")]
mod metrics_impl {
    use once_cell::sync::Lazy;
    use opentelemetry::{KeyValue, global};

    static STATE_OP_COUNT: Lazy<opentelemetry::metrics::Counter<u64>> = Lazy::new(|| {
        global::meter("greentic-telemetry")
            .u64_counter("greentic.state.op.count")
            .with_description("Total number of state KV operations")
            .build()
    });

    static STATE_OP_DURATION: Lazy<opentelemetry::metrics::Histogram<f64>> = Lazy::new(|| {
        global::meter("greentic-telemetry")
            .f64_histogram("greentic.state.op.duration_ms")
            .with_description("State KV operation duration in milliseconds")
            .build()
    });

    pub fn record(op_name: &str, status: &str, duration_ms: f64) {
        let attrs = [
            KeyValue::new("greentic.state.op", op_name.to_string()),
            KeyValue::new("greentic.state.status", status.to_string()),
        ];
        STATE_OP_COUNT.add(1, &attrs);
        STATE_OP_DURATION.record(duration_ms, &attrs);
    }
}

/// Record state-specific metrics (counter + histogram).
///
/// `op_name` is one of `state.get`, `state.put`, `state.delete`, `state.list`, `state.cas`.
/// `status` is one of `hit`, `miss`, `ok`, `err`, `denied`.
#[cfg(feature = "otlp")]
pub fn record_state_metric(op_name: &str, status: &str, duration_ms: f64) {
    metrics_impl::record(op_name, status, duration_ms);
}

/// No-op when the `otlp` feature is disabled.
#[cfg(not(feature = "otlp"))]
pub fn record_state_metric(_op_name: &str, _status: &str, _duration_ms: f64) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_key_deterministic() {
        let h1 = hash_key_for_audit("user:123:prefs");
        let h2 = hash_key_for_audit("user:123:prefs");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn hash_key_differs_for_different_keys() {
        let h1 = hash_key_for_audit("key_a");
        let h2 = hash_key_for_audit("key_b");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_key_empty_string() {
        let h = hash_key_for_audit("");
        assert_eq!(h.len(), 16);
    }

    #[test]
    fn emit_state_pre_noop_when_disabled() {
        let config = crate::OperationSubsConfig {
            enabled: false,
            ..Default::default()
        };
        // Should not panic
        emit_state_op_pre(
            &config,
            "state.get",
            "dev::t1::team",
            "abc123",
            "t1",
            "team",
        );
    }

    #[test]
    fn emit_state_post_noop_when_disabled() {
        let config = crate::OperationSubsConfig {
            enabled: false,
            ..Default::default()
        };
        emit_state_op_post(
            &config,
            "state.put",
            "dev::t1::team",
            "abc123",
            "t1",
            "team",
            "ok",
            1.5,
        );
    }

    #[test]
    fn emit_state_post_skips_denied_when_excluded() {
        let config = crate::OperationSubsConfig {
            enabled: true,
            include_denied: false,
            ..Default::default()
        };
        emit_state_op_post(
            &config,
            "state.put",
            "ns",
            "hash",
            "t1",
            "team",
            "denied",
            0.5,
        );
    }

    #[test]
    fn emit_state_post_allows_denied_when_included() {
        let config = crate::OperationSubsConfig {
            enabled: true,
            include_denied: true,
            ..Default::default()
        };
        emit_state_op_post(
            &config,
            "state.put",
            "ns",
            "hash",
            "t1",
            "team",
            "denied",
            0.5,
        );
    }

    #[test]
    fn emit_state_pre_runs_when_enabled() {
        let config = crate::OperationSubsConfig::default();
        // Should not panic
        emit_state_op_pre(&config, "state.get", "ns", "hash", "t1", "team");
    }

    #[test]
    fn emit_state_post_runs_when_enabled() {
        let config = crate::OperationSubsConfig::default();
        emit_state_op_post(&config, "state.get", "ns", "hash", "t1", "team", "hit", 0.1);
    }
}
