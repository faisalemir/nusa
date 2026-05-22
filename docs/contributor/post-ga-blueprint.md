# Post-GA blueprint (P5 / M6)

Items deferred from v1.0 GA. Track here instead of implying they ship in `0.1.0`.

## Feature flags (recommended)

| Feature | Config sketch | Status |
|---------|---------------|--------|
| ACME auto-HTTPS | `tls.acme.enabled = false` | Partial code in `nusa-gateway` |
| HTTP/3 QUIC | `quic.enabled = false` | Experimental |
| Redis broadcast | Bring-your-own or future crate | Not GA |
| Blue-green deploy | `nusa deploy` | Stub / partial `bluegreen.rs` |
| `nusa install` (Rust Composer) | CLI placeholder | M6 |
| Multi-version PHP pools | — | M6 |
| K8s operator | — | M6 |

## Documentation rules

- Do not document `/admin/recycle-all` until implemented in `nusa-gateway`.
- Mark QUIC/ACME as **experimental** in public docs when enabled.
- WASM engine remains dev-only unless a real module loader ships.

## When to revisit

After `just podman-ci-e2e` is green on the release tag and [`docs/public/production-status.md`](../public/production-status.md) marks Phase 5 complete.
