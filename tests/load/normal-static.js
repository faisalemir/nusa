/**
 * P3 load smoke — static/simple response (configure target URL via K6_TARGET).
 * Run: k6 run tests/load/normal-static.js
 */
import http from 'k6/http';
import { check } from 'k6';

const target = __ENV.K6_TARGET || 'http://127.0.0.1:8080/';

export const options = {
  vus: Number(__ENV.K6_VUS || 50),
  duration: __ENV.K6_DURATION || '30s',
  thresholds: {
    http_req_failed: ['rate<0.01'],
    http_req_duration: ['p(99)<500'],
  },
};

export default function () {
  const res = http.get(target);
  check(res, {
    'status is 200': (r) => r.status === 200,
  });
}
