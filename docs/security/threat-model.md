# Threat Model v1.0

## 1. System Overview
The Nusa Runtime consists of:
- **Gateway (Rust):** Handles HTTP/TLS, routing, middleware.
- **Engine (Rust/PHP):** Executes PHP code via FFI, WASM, or Process.
- **Worker Pool (PHP):** Long-running Laravel workers (Octane mode).
- **IPC Layer:** Unix Sockets for communication between Gateway and Workers.

## 2. Trust Boundaries
| Boundary | Trust Level | Description |
|----------|-------------|-------------|
| Client -> Gateway | Untrusted | Public internet, TLS termination. |
| Gateway -> Engine | Trusted | Internal loopback/Unix socket. |
| Engine -> FS | Restricted | Landlock RO for code, RW for tmp. |
| Worker -> DB/Cache | Trusted | Internal network, authenticated. |

## 3. Attack Vectors and Mitigations

### 3.1 IPC Injection / Framing Abuse
- **Vector:** Malicious payload sent via Unix Socket to crash worker or execute arbitrary code.
- **Mitigation:** Length-prefixed framing, strict schema validation, version handshake, timeout on read/write.
- **Verification:** Property-based testing, fuzzing IPC parser.

### 3.2 VFS Escape / Path Traversal
- **Vector:** PHP script attempts to access files outside /app/public.
- **Mitigation:** Landlock rules (RO for code dir, RW for tmp dir), strict symlink resolution, per-tenant VFS mounts.
- **Verification:** Penetration audit, multi-tenant leak suite.

### 3.3 Memory Exhaustion / DoS
- **Vector:** Large request body, infinite loop, memory leak in PHP.
- **Mitigation:** Request size caps, RSS monitoring per worker, circuit breaker, automatic worker recycle on OOM risk.
- **Verification:** Load testing, OOM simulation, HPA validation.

### 3.4 Supply Chain Compromise
- **Vector:** Malicious crate dependency or compromised build pipeline.
- **Mitigation:** Pinned dependencies, `just audit` in CI, SBOM generation, SLSA provenance.
- **Verification:** CI attestation, signed releases.

### 3.5 State Cross-Bleed (Octane Mode)
- **Vector:** Data from one request leaks into another due to persistent state.
- **Mitigation:** Explicit state reset via Octane events, container rebinds, superglobal emulation from IPC payload.
- **Verification:** Automated leak detection suite (10k concurrent requests).

## 4. Compliance Mapping
| Standard | Requirement | Implementation |
|----------|-------------|----------------|
| SOC 2 Type II | Audit Logs | Structured JSON logs with trace_id, immutable storage. |
| GDPR | Data Isolation | Per-tenant VFS, cache key prefixing, right-to-erase hooks. |
| HIPAA | Encryption | TLS 1.3 enforced, encryption at rest for DB/Cache. |
| ISO 27001 | Vulnerability Mgmt | CVE monitoring, patch SLAs, regular penetration tests. |
