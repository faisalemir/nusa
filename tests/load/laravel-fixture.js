/**
 * P3 load smoke — Laravel minimal fixture routes (Octane pool or gateway).
 * Run against a running `nusa` instance with fixture `code_dir`.
 */
import http from 'k6/http';
import { check } from 'k6';

const base = __ENV.K6_BASE || 'http://127.0.0.1:8080';

export const options = {
  vus: Number(__ENV.K6_VUS || 20),
  duration: __ENV.K6_DURATION || '30s',
};

export default function () {
  const ping = http.get(`${base}/nusa-ping`);
  check(ping, { 'ping 200': (r) => r.status === 200 && r.body.includes('pong') });

  const echo = http.post(`${base}/nusa-echo`, 'tenant=data', {
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
  });
  check(echo, { 'echo 200': (r) => r.status === 200 });
}
