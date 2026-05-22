# Observability artifacts

Grafana dashboard JSON and runbook links for operating Nusa in production.

## Dashboards

| File | Focus |
|------|--------|
| [../monitoring/runtime-overview.json](../monitoring/runtime-overview.json) | Gateway requests, errors, latency |
| [../monitoring/worker-pool.json](../monitoring/worker-pool.json) | Octane pool (when enabled) |
| [../monitoring/ipc-layer.json](../monitoring/ipc-layer.json) | `nusa_ipc_latency_ms` histogram |
| [../monitoring/laravel-app.json](../monitoring/laravel-app.json) | App-level panels (template) |

Import into Grafana 10+; set Prometheus datasource to scrape `GET /metrics` on the Nusa bind address.

## Metrics (Rust)

Defined in `nusa-telemetry` — see `NusaMetrics` and Prometheus exporter in `nusa-cli` startup.

## Tracing

Gateway extracts W3C `traceparent` in middleware; Octane IPC requests attach `TraceContext` on `IpcMessage::Request`. Verify: `nusa-e2e-tests` `trace_propagation_test`.

## Operations

- [Public runbook](../public/operations/runbook.md)
- Legacy path: [../sre/runbook.md](../sre/runbook.md) (redirect stub)
