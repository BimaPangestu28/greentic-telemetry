# Correlation Model

## Span Hierarchy

Every provider operation produces a single trace with the following structure:

```
greentic.op  (root span)
│
├── [event] operation.requested       ← pre-execution event
│
├── greentic.op.hooks (pre)           ← pre-hook chain span
│   ├── greentic.op.hook              ← individual hook
│   └── greentic.op.hook              ← individual hook
│
├── greentic.op.pack_resolve          ← catalog lookup
│
├── greentic.op.card_render           ← adaptive card rendering
│
├── greentic.op.runner_exec           ← flow execution (if entry flow)
│   OR
├── greentic.op.component_invoke      ← direct WASM invocation
│
├── greentic.op.hooks (post)          ← post-hook chain span
│   └── greentic.op.hook
│
├── [event] operation.completed       ← post-execution event
│   OR
├── [event] operation.error           ← error event (if failed)
│
└── (root span closes with duration_ms recorded)
```

## Root Span (`greentic.op`)

The root span is created at the beginning of `invoke_provider_op` or
`invoke_capability` and wraps the entire operation lifecycle:

```rust
let root_span = operation_root_span(op_id, provider_type, &ctx.tenant, &team);
let _guard = root_span.enter();
```

All child spans and events are automatically parented to this root span.

Fields recorded on the root span at completion:

| Field | When Set |
|-------|----------|
| `otel.status_code` | Always (`OK` or `ERROR`) |
| `greentic.op.duration_ms` | Always |
| `greentic.meta.routing.provider` | After pack resolves |
| `error.type` | On error (`denied` or `invoke_error`) |
| `error.message` | On error |

## Events vs Child Spans

**Events** are lightweight annotations on the root span — they appear as
log entries within the span in Jaeger/Grafana. Used for:

- `operation.requested` — marks when execution starts
- `operation.completed` — marks completion with status and duration
- `operation.error` — marks an error with type and message

**Child spans** represent measurable units of work with their own duration:

- `pack_resolve` — how long the catalog lookup takes
- `card_render` — how long card rendering takes
- `runner_exec` — how long the flow/runner execution takes
- `component_invoke` — how long the WASM component invocation takes
- `hooks` / `hook` — how long hook evaluation takes

## Denied Operations

When a pre-hook denies an operation, the trace still contains:

```
greentic.op  (root, status=ERROR)
├── [event] operation.requested
├── greentic.op.hooks (pre)
│   └── greentic.op.hook
├── [event] operation.error  (error.type="denied")
└── [event] operation.completed  (status="denied")
```

This ensures denied operations are visible in the observability backend.
The `include_denied_ops: false` config suppresses the `operation.completed`
event but the error event is always emitted.

## Capability Invocations

Capability invocations (`invoke_capability`) follow the same correlation
model. The root span is named `cap.invoke:{cap_id}` instead of the bare
operation ID:

```
greentic.op  (cap.invoke:greentic.cap.telemetry.v1)
├── [event] operation.requested
├── greentic.op.hooks (pre)
├── greentic.op.component_invoke
├── greentic.op.hooks (post)
└── [event] operation.completed
```

## Metrics Correlation

Metrics use the same dimension labels as spans, enabling cross-referencing:

- **Trace → Metric**: Filter Prometheus by `greentic_op_name="send_payload"`
  to see the same operations that appear as spans in Jaeger.
- **Metric → Trace**: From a spike in `greentic_operation_error_count`,
  search for traces with `error.type` set in the same time window.

## Multi-Tenant Isolation

Each root span carries `greentic.tenant.id` and `greentic.team.id`,
enabling per-tenant trace filtering. With `hash_ids: true`, these become
blake3 hashes — still unique per tenant but not revealing the raw ID.

## Example: Viewing in Jaeger

1. Search by service `greentic-operator`
2. Filter by tag `greentic.op.name = send_payload`
3. Open a trace to see the root `greentic.op` span
4. Expand to see child spans (`pack_resolve`, `runner_exec`, etc.)
5. Click on the root span to see events (`operation.requested`, `operation.completed`)
6. Failed traces show `otel.status_code = ERROR` with `error.type` and `error.message`
