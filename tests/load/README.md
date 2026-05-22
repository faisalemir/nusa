# Load tests (P3)

k6 and wrk scenarios for Normal Mode vs FPM baselines. Run on the same host as Nusa for comparable numbers.

## Prerequisites

- Nusa listening on `:8080` with `engine = child`, `octane_workers = 0`
- Optional: nginx + php-fpm baseline on `:8081`

## wrk (quick)

```bash
wrk -t4 -c100 -d30s http://127.0.0.1:8080/
```

Record P50/P99 from output; paste into [`docs/benchmarks/normal-mode-report.md`](../../docs/benchmarks/normal-mode-report.md).

## k6 (script placeholder)

Create `normal-static.js` when promoting to CI:

```javascript
import http from 'k6/http';
import { check } from 'k6';
export const options = { vus: 50, duration: '30s' };
export default function () {
  const res = http.get('http://127.0.0.1:8080/');
  check(res, { 'status is 200': (r) => r.status === 200 });
}
```

Run: `k6 run tests/load/normal-static.js`

## Sign-off

Results belong in `docs/benchmarks/normal-mode-report.md` before GA v1.0.0 tag.
