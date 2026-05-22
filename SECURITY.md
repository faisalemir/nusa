# Security Policy

## 1. Safety First Principle
- **Global Rule:** `#![deny(unsafe_code)]` is enforced in all crates except `nusa-engine-ffi`.
- **FFI Boundary:** All `unsafe` blocks in `nusa-engine-ffi` must have a `// SAFETY:` comment explaining why it is sound, referencing ZTS guarantees and isolation mechanisms.
- **Audit:** `just audit` is run in CI to visualize unsafe usage.

## 2. Threat Model

Full detail: [docs/security/threat-model.md](docs/security/threat-model.md).

| Vector | Mitigation | Verification |
|--------|------------|--------------|
| **IPC Injection** | Length-prefixed framing, strict schema validation, version handshake | Property-based testing, chaos engineering |
| **VFS Escape** | Landlock RO FS, strict symlink resolution, per-tenant VFS mounts | Penetration audit, multi-tenant leak suite |
| **Supply Chain** | Pinned deps, SBOM generation, SLSA provenance, signed releases | `cargo audit/deny`, CI attestation |
| **Memory Exhaustion** | RSS caps, circuit breaker, backpressure, request size limits | Load testing, OOM simulation |

Contributor security development: [docs/contributor/security-dev.md](docs/contributor/security-dev.md).

## 3. Compliance Mapping
- **SOC 2 Type II:** Immutable audit logs, RBAC matrix, incident runbooks.
- **GDPR:** Data isolation per tenant, right-to-erase hooks, TLS 1.3 enforcement.
- **HIPAA:** Strict isolation, minimal attack surface, encryption at rest/transit.

## 4. Reporting Vulnerabilities
Please report security vulnerabilities to security@nusa.dev. Do not open public issues.
