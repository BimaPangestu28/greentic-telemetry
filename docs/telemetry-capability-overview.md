# Telemetry Capability Overview

## What is the Telemetry Capability?

The Greentic platform supports a **capability-based** telemetry model. Rather than
hard-coding an observability backend, operators install a **telemetry provider pack**
(e.g. `telemetry-otlp`) that configures the OpenTelemetry pipeline at runtime.

The capability contract is `greentic.cap.telemetry.v1`.

## How It Works

```
┌──────────────────────┐      ┌──────────────────────┐
│  telemetry-otlp pack │      │   greentic-operator   │
│                      │      │                       │
│  setup.yaml ─────────┼──QA──┤  Wizard collects      │
│                      │      │  answers, seeds secrets│
│  component ──────────┼─op───┤  telemetry.configure   │
│  (WASM)              │      │  → TelemetryProvider-  │
│                      │      │    Config (JSON)       │
│  pack.yaml           │      │                       │
│  extensions:         │      │  init_from_provider_   │
│    capabilities.v1   │      │  config() → OTel SDK  │
└──────────────────────┘      └───────────────────────┘
```

### Resolution Flow

1. Operator discovers packs with `greentic.ext.capabilities.v1` extension
2. Resolves the offer with `cap_id = "greentic.cap.telemetry.v1"`
3. If the offer has `requires_setup: true`, runs the QA wizard via `qa_ref`
4. Invokes the provider component with op `telemetry.configure`
5. The component returns a `TelemetryProviderConfig` (JSON)
6. Operator calls `init_from_provider_config()` to set up the OTel pipeline

### Capability Extension Schema

In `pack.yaml`:

```yaml
extensions:
  greentic.ext.capabilities.v1:
    kind: greentic.ext.capabilities.v1
    version: 0.4.0
    inline:
      schema_version: 1
      offers:
        - offer_id: "telemetry-otlp-v1"
          cap_id: "greentic.cap.telemetry.v1"
          version: "v1"
          provider:
            component_ref: "component-telemetry-provider"
            op: "telemetry.configure"
          priority: 100
          requires_setup: true
          setup:
            qa_ref: "setup.yaml"
```

### Key Fields

| Field | Description |
|-------|-------------|
| `offer_id` | Unique ID for this particular offer |
| `cap_id` | The capability being provided (`greentic.cap.telemetry.v1`) |
| `provider.component_ref` | Which WASM component handles the op |
| `provider.op` | The operation name invoked on the component |
| `priority` | Higher = preferred when multiple offers exist |
| `requires_setup` | Whether QA wizard must run before first use |
| `setup.qa_ref` | Path to `setup.yaml` within the pack |

## Setup Wizard

The `setup.yaml` defines questions the operator answers during installation:

- **preset** — Backend preset (honeycomb, datadog, jaeger, etc.)
- **otlp_endpoint** — OTLP collector URL
- **otlp_api_key** — API key (stored as a secret)
- **export_mode** — `otlp-grpc` | `otlp-http` | `json-stdout`
- **sampling_ratio** — Trace sampling (0.0–1.0)
- **min_log_level** — Minimum log level filter
- **enable_operation_subs** — Toggle operation subscription telemetry
- **include_denied_ops** — Include denied ops in telemetry
- **include_team_in_metrics** — Add team_id to metric labels
- **hash_ids** — Hash tenant/team IDs (privacy)

## TelemetryProviderConfig

The WASM component returns this JSON to the operator:

```json
{
  "export_mode": "otlp-grpc",
  "endpoint": "http://collector:4317",
  "headers": { "x-api-key": "from-secrets" },
  "sampling_ratio": 1.0,
  "compression": "gzip",
  "service_name": "greentic-operator",
  "preset": "jaeger",
  "enable_operation_subs": true,
  "operation_subs_mode": "metrics_and_traces",
  "include_denied_ops": true,
  "payload_policy": "hash_only",
  "min_log_level": "info",
  "exclude_ops": ["healthcheck"],
  "drop_payloads": false,
  "tenant_attribution": {
    "include_tenant": true,
    "include_team": true,
    "include_team_in_metrics": false,
    "hash_ids": false
  }
}
```

## Validation

After receiving the config, the operator validates it via
`validate_telemetry_config()`, which returns advisory warnings for:

- Unknown export_mode, compression, subs_mode, payload_policy, log_level
- Missing endpoint for OTLP modes (unless a preset provides one)
- Sensitive headers that should be secrets-backed
- Sampling ratio out of [0.0, 1.0]
- Empty redaction patterns
- Mismatched TLS cert/key
- hash_ids enabled with both include_tenant and include_team disabled
