# Nusa Octane-Safe Package Guidelines

This document defines the criteria for Laravel packages to be certified **Octane-Safe** on the Nusa runtime.

---

## Anti-Pattern Registry

The following patterns are known to cause issues in long-running worker processes:

| Anti-Pattern | Problem | Fix |
|-------------|---------|-----|
| `static` state in service providers | Persists across requests | Use `app()->make()` instead |
| Singleton containers without reset | Container retains resolved bindings | Flush on `RequestTerminated` event |
| File handle caching | FD leaks across requests | Close/refresh per request |
| Global variable mutations | Cross-request data bleed | Use request-scoped storage |
| PDO persistent connections without pool | Connection exhaustion | Use connection pooling or reconnect per request |
| Event listeners registered in `boot()` | Duplicate listeners per request | Check `already_booted` flag |
| `Carbon::setTestNow()` in production | Freezes time globally | Use scoped time freezing |

---

## Octane-Safe Badge Criteria

A package earns the **Octane-Safe** badge when it meets ALL of the following:

1. **No static mutable state** — verified by static analysis scan
2. **Implements `OctaneContract`** — responds to lifecycle events
3. **Passes leak test suite** — 10k sequential requests with zero state bleed
4. **Clean memory profile** — RSS delta <= 2% over 50k requests
5. **Graceful shutdown** — responds to SIGTERM within 5 seconds

---

## CI Preset Configuration

Add this to your package's CI pipeline:

```yaml
# .github/workflows/octane-safe.yml
name: Octane-Safe Validation

on: [push, pull_request]

jobs:
  octane-safe:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Nusa Runtime
        run: cargo install --git https://github.com/nusa-rs/nusa --branch main nusa
      - name: Run leak test
        run: nusa test --package . --iterations 10000
      - name: Memory profile
        run: nusa test --package . --memory-profile --threshold 2
      - name: Static analysis
        run: nusa test --package . --static-analysis
```

---

## Migration Checklist

When migrating a package from FPM to Nusa Octane:

- [ ] Remove all `static` mutable variables
- [ ] Replace singletons with factory/resolver patterns
- [ ] Add `RequestReceived` / `RequestTerminated` event handlers
- [ ] Test with `nusa test --leak-check`
- [ ] Verify RSS stability under load
- [ ] Update composer.json with `"nusa-rs/octane-safe": true`
