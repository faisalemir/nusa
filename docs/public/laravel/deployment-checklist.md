# Deployment checklist (Laravel)

Use this list before sending production traffic to Nusa.

---

## Pre-staging

- [ ] Read [Production status](../production-status.md) and [Compatibility matrix](../compatibility-matrix.md)  
- [ ] Choose **Normal** vs **Octane** mode for this app  
- [ ] [PHP driver](php-driver.md) installed if using Octane  
- [ ] `nusa.toml` reviewed: `code_dir`, `vfs_root`, `tmp_dir`, limits  

---

## Staging (Alpine-shaped)

- [ ] Image based on **Alpine musl** (or equivalent to `nusa-test-runner`)  
- [ ] `composer install --no-dev` in build  
- [ ] `php artisan config:cache` / `route:cache` / `view:cache` as appropriate  
- [ ] `storage/` and `bootstrap/cache/` writable on mounted volumes  
- [ ] `nusa` starts with **`octane_workers`** config matching prod  
- [ ] `/health` and `/ready` probed by orchestrator  
- [ ] `/metrics` scraped by Prometheus  
- [ ] Load test critical routes (p50/p95 latency, error rate)  
- [ ] Run `just podman-ci` or equivalent CI on release branch  

---

## Octane-specific

- [ ] `/ready` returns **200** only when pool healthy  
- [ ] Startup **fails** if workers cannot spawn (intentional—fix before deploy)  
- [ ] Sessions: `redis` or database—not file sessions on multi-replica unless shared storage  
- [ ] Worker recycle limits tuned (`octane_max_memory_mb`, `octane_max_requests`)  
- [ ] Leak suite passed in pre-GA: `just podman-ci-e2e`  

---

## Production cutover

- [ ] Blue/green or canary with `/ready` gate  
- [ ] Alerts on 5xx rate, `/ready` failures, worker recycle churn  
- [ ] Runbook link shared with on-call: [Operations runbook](../operations/runbook.md)  
- [ ] Rollback path documented (revert image + `nusa.toml`)  

---

## Post-launch

- [ ] Compare metrics to previous FPM/RoadRunner baseline  
- [ ] Record benchmark numbers in [Normal mode report](../benchmarks/normal-mode-report.md) when available  

---

## Related

- [Migration](../migration.md)  
- [Troubleshooting](troubleshooting.md)  
