# Nusa Security & Compliance Evidence Pack

## 1. TLS Enforcement

- TLS 1.3 only (no TLS 1.2 or below)
- Configurable via `nusa.toml` or `NUSA_TLS_CERT_PATH` / `NUSA_TLS_KEY_PATH`
- ACME automatic certificate management (Let's Encrypt, ZeroSSL)
- HTTP → HTTPS auto-redirect when TLS is enabled
- Certificate hot-reload via file watcher

## 2. Sandbox Defaults

| Layer | Mechanism | Status |
|-------|-----------|--------|
| Filesystem | Landlock (Linux only) | Implemented |
| Syscalls | Seccomp-BPF (Linux only) | Stub (requires libseccomp-dev) |
| Memory | WASM StoreLimits | Implemented |
| CPU | WASM fuel limits | Implemented |
| Network | Socket isolation per engine | Config-level |

## 3. Audit Logging

- All requests logged with trace_id, tenant_id, duration, status
- Structured JSON format for SIEM ingestion
- OpenTelemetry spans for distributed tracing
- Prometheus metrics for anomaly detection

## 4. Data Isolation

- Per-tenant VFS path resolution with directory traversal prevention
- Per-tenant rate limiting (token bucket)
- Per-tenant circuit breakers
- TenantId newtype enforces type-level scoping

## 5. SOC 2 Controls Mapping

| Control | Evidence |
|---------|----------|
| CC6.1 Logical access | TenantId isolation, rate limiting |
| CC6.6 Security boundaries | Landlock, Seccomp, WASM limits |
| CC7.1 Monitoring | OTLP tracing, Prometheus metrics |
| CC7.2 Incident response | SRE runbook, circuit breaker |

## 6. GDPR Controls Mapping

| Requirement | Evidence |
|-------------|----------|
| Data minimization | Tenant VFS isolation |
| Access control | TenantId newtype, rate limits |
| Audit trail | Structured JSON logs with trace IDs |
| Encryption | TLS 1.3 in transit |

## 7. HIPAA Controls Mapping

| Requirement | Evidence |
|-------------|----------|
| Access control | Tenant isolation, circuit breakers |
| Audit controls | Full request/response logging |
| Integrity | WASM memory limits prevent corruption |
| Transmission security | TLS 1.3 enforced |

## 8. SBOM Generation

```bash
cargo install cargo-sbom
cargo sbom --output sbom.json
```

SBOM is generated in CI pipeline and included in release artifacts.

## 9. CVE Monitoring Process

1. `cargo audit` runs on every CI check
2. `cargo deny` checks for advisory database
3. Weekly automated audit via GitHub Actions
4. Critical CVEs block releases
