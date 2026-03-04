# Operation Subscription Telemetry

## Overview

Operation subscriptions ("op subs") emit structured telemetry for every
provider operation in the Greentic pipeline. This gives full visibility into
what operations run, how long they take, and whether they succeed or fail —
without leaking sensitive payload data.

## What Gets Emitted

### Traces (Spans + Events)

Each operation produces:

| Telemetry Item | Type | Description |
|----------------|------|-------------|
| `greentic.op` | Root span | Wraps the entire operation lifecycle |
| `operation.requested` | Event on root span | Fired before execution starts |
| `operation.completed` | Event on root span | Fired after execution completes |
| `operation.error` | Event on root span | Fired on error (denied or invoke failure) |
| `greentic.op.pack_resolve` | Child span | Catalog lookup for provider pack |
| `greentic.op.card_render` | Child span | Adaptive Card rendering (if applicable) |
| `greentic.op.runner_exec` | Child span | Runner/flow execution |
| `greentic.op.component_invoke` | Child span | Direct WASM component invocation |
| `greentic.op.hooks` | Child span | Hook chain evaluation |
| `greentic.op.hook` | Child span | Individual hook binding execution |

### Root Span Fields

| Field | Type | Description |
|-------|------|-------------|
| `greentic.op.name` | string | Operation ID (e.g. `send_payload`) |
| `greentic.provider.type` | string | Provider type (e.g. `messaging.telegram`) |
| `greentic.tenant.id` | string | Tenant identifier (may be hashed) |
| `greentic.team.id` | string | Team identifier (may be hashed) |
| `otel.status_code` | string | `OK` or `ERROR` |
| `error.type` | string | Error classification (e.g. `denied`, `invoke_error`) |
| `error.message` | string | Human-readable error description |
| `greentic.meta.routing.provider` | string | Resolved provider after pack lookup |
| `greentic.meta.classification` | string | Reserved for future operation classification |
| `greentic.op.duration_ms` | f64 | Total operation duration in milliseconds |

### Metrics

| Metric | Type | Labels |
|--------|------|--------|
| `greentic.operation.duration_ms` | Histogram | op_name, provider_type, status, tenant_id [, team_id] |
| `greentic.operation.count` | Counter | op_name, provider_type, status, tenant_id [, team_id] |
| `greentic.operation.error_count` | Counter | op_name, provider_type, error_code, tenant_id [, team_id] |

## Payload Safety Defaults

By default, **no payload content** appears in telemetry. The `payload_policy`
controls what (if anything) is included:

| Policy | Payload Content | Payload Size | Payload Hash |
|--------|:-:|:-:|:-:|
| `none` (default) | - | - | - |
| `hash_only` | - | yes | yes (blake3 hex) |

The `drop_payloads: true` flag is an absolute override that forces `none`
regardless of `payload_policy`.

## Tenant Attribution

Controls which tenant/team identifiers appear in spans and metrics:

| Setting | Default | Description |
|---------|---------|-------------|
| `include_tenant` | `true` | Include `greentic.tenant.id` in spans |
| `include_team` | `true` | Include `greentic.team.id` in spans |
| `include_team_in_metrics` | `false` | Include `greentic.team.id` in metric labels |
| `hash_ids` | `false` | Hash IDs with blake3 before emitting |

Setting `hash_ids: true` enables privacy-preserving correlation — you can
still group traces by tenant, but raw IDs don't appear in your backend.

## How to Enable/Disable

### Full Disable

```json
{
  "enable_operation_subs": false
}
```

No events, spans, or metrics are emitted for operations.

### Metrics Only (No Traces)

```json
{
  "enable_operation_subs": true,
  "operation_subs_mode": "metrics_only"
}
```

Counters and histograms are recorded, but no trace events are emitted.

### Traces Only (No Metrics)

```json
{
  "enable_operation_subs": true,
  "operation_subs_mode": "traces_only"
}
```

### Exclude Specific Operations

```json
{
  "exclude_ops": ["healthcheck", "ping"]
}
```

Named operations are silently skipped in both traces and metrics.

### Include/Exclude Denied Operations

```json
{
  "include_denied_ops": false
}
```

When `false`, operations denied by pre-hooks do not emit `operation.completed`
events (the `operation.requested` event still fires).

## Recommended Defaults

```json
{
  "enable_operation_subs": true,
  "operation_subs_mode": "metrics_and_traces",
  "payload_policy": "hash_only",
  "include_denied_ops": true,
  "drop_payloads": false,
  "tenant_attribution": {
    "include_tenant": true,
    "include_team": true,
    "include_team_in_metrics": false,
    "hash_ids": false
  }
}
```

This gives strong platform observability without accidental data leaks.
Enable `hash_ids` in production if tenant IDs are PII-sensitive.
