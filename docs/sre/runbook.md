//! SRE Runbook: Incident playbooks for Nusa PHP Runtime
//!
//! Skills applied:
//! - `domain-cloud-native`: Incident response, observability-driven debugging
//! - `m14-mental-model`: Document the "why" behind each playbook

---

# Nusa PHP Runtime — SRE Runbook v1

## 1. Incident: Memory Leak / OOM

### Symptoms
- `nusa_worker_rss_mb` metric trending upward over time
- OOM kills in Kubernetes/Docker logs
- Increased latency due to GC pauses
- `nusa_worker_recycles_total` spiking

### Containment
1. **Trigger immediate worker recycle:**
   ```bash
   curl -X POST http://admin-host:9090/admin/recycle-all
   ```
2. **Reduce `max_requests` temporarily via hot-reload:**
   ```bash
   # Edit nusa.toml
   max_requests = 250  # was 1000
   ```
3. **Kill runaway workers (emergency):**
   ```bash
   systemctl restart nusa
   ```

### Root Cause Analysis
1. **Check Grafana "Worker Pool" dashboard** — look for:
   - RSS drift per worker (should reset after recycle)
   - Request duration correlation with memory
2. **Inspect Laravel packages** that use:
   - Static caches that grow unbounded
   - Image processing without memory limits
   - Database query log accumulation
3. **Update OctaneRustServiceProvider** to flush identified services:
   ```php
   Octane::listen(RequestReceived::class, function ($event) {
       $event->sandbox->make('cache')->flush();
       $event->sandbox->make('db')->flushQueryLog();
   });
   ```

### Resolution
1. Add missing service flush to Octane event listener
2. Deploy hotfix with reduced `max_requests` as safety net
3. Monitor RSS for 1 hour — should be flat after fix

---

## 2. Incident: IPC Breakdown

### Symptoms
- `nusa_ipc_latency_ms` spike (P99 > 100ms)
- `nua_errors_total` increasing
- 502 Bad Gateway errors
- Workers stuck in `busy` state

### Containment
1. **Drain worker pool:**
   ```bash
   curl -X POST http://admin-host:9090/admin/drain
   ```
2. **Restart runtime pods/nodes** (rolling restart):
   ```bash
   kubectl rollout restart deployment/nusa
   ```

### Root Cause Analysis
1. **Verify Unix Socket permissions:**
   ```bash
   ls -la /app/.octane/worker-*.sock
   # Should be owned by nusa:nusa with 0660
   ```
2. **Check IPC contract version compatibility:**
   - Rust side: `nusa-ipc` version
   - PHP side: `php-driver` version
3. **Review logs for framing errors:**
   ```bash
   journalctl -u nusa | grep "framing\|IPC"
   ```

### Resolution
1. Fix socket permissions or path configuration
2. Ensure Rust and PHP driver versions match
3. Restart workers and verify IPC latency returns to <10ms P99

---

## 3. Incident: State Cross-Bleed (Octane Mode)

### Symptoms
- Users seeing other users' data
- Session mix-ups across requests
- Cache keys returning wrong data
- Database connections holding wrong auth state

### Containment
1. **Force full pool recycle:**
   ```bash
   curl -X POST http://admin-host:9090/admin/recycle-all
   ```
2. **Invalidate all sessions in Redis:**
   ```bash
   redis-cli FLUSHDB
   ```

### Root Cause Analysis
1. **Audit `RequestReceived` listeners** — verify:
   - All facades are flushed
   - Container singletons are rebound
   - DB connection is pinged/reconnected
   - Session is cleared
2. **Check for global statics** in controllers/services:
   ```bash
   grep -r "static \$" app/ --include="*.php"
   ```
3. **Run leak detection suite in staging:**
   ```bash
   php artisan octane:test --server=rust --requests=10000
   ```

### Resolution
1. Add missing service flush/reset to OctaneRustServiceProvider
2. Deploy fix to production
3. Run 10k concurrent request test in staging before cutover
4. Monitor for 24 hours — zero cross-bleed reports

---

## 4. Incident: Traffic Spike / Circuit Breaker Open

### Symptoms
- `nusa_requests_total` suddenly spikes 10x+
- Circuit breaker status: `OPEN`
- Users receiving 503 Service Unavailable
- `nusa_worker_pool_size` at max capacity

### Containment
1. **Check circuit breaker auto-recovery:**
   - Breaker should transition to HALF-OPEN after 30s
   - If not, check for persistent upstream failures
2. **Scale horizontally (if K8s):**
   ```bash
   kubectl scale deployment nusa --replicas=10
   ```

### Root Cause Analysis
1. **Check Grafana "Runtime Overview" dashboard:**
   - Request rate vs historical baseline
   - Error rate by type
   - Worker pool utilization
2. **Determine if traffic is legitimate or DDoS:**
   - Check source IPs
   - Check request patterns
3. **If DDoS — enable WAF rules:**
   ```bash
   kubectl apply -f waf-rules.yaml
   ```

### Resolution
1. If legitimate: scale up and monitor
2. If DDoS: activate WAF and block malicious IPs
3. Post-incident: tune circuit breaker thresholds

---

## 5. Incident: TLS Certificate Expiry

### Symptoms
- `nusa_tls_errors_total` increasing
- Clients reporting SSL errors
- Health checks failing on HTTPS endpoint

### Containment
1. **Rotate certificate immediately:**
   ```bash
   cp /certs/new-cert.pem /etc/nusa/tls/cert.pem
   cp /certs/new-key.pem /etc/nusa/tls/key.pem
   systemctl reload nusa
   ```

### Prevention
1. **Set up cert-manager alerts** for expiry < 30 days
2. **Add Prometheus alert rule:**
   ```yaml
   alert: TLSCertExpiringSoon
   expr: tls_cert_expiry_days < 30
   for: 1h
   ```

---

## 6. Incident: Config Hot-Reload Failure

### Symptoms
- `nusa_config_reload_errors_total` increasing
- Log: "config reload failed: parse error"
- Old config still in use after file change

### Containment
1. **Validate config file syntax:**
   ```bash
   nusa config validate --config /etc/nusa/nusa.toml
   ```
2. **Restart runtime if config is stuck:**
   ```bash
   systemctl restart nusa
   ```

### Root Cause Analysis
1. **Check config file for TOML syntax errors**
2. **Verify file permissions** (nusa user must have read access)
3. **Check notify watcher** for dropped events

### Resolution
1. Fix TOML syntax and redeploy
2. Ensure hot-reload watcher is healthy
3. Add config validation to CI pipeline
