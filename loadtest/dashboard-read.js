import http from 'k6/http';
import { check, sleep } from 'k6';
import { URL } from 'https://jslib.k6.io/url/1.0.0/index.js';

const BASE_URL = __ENV.K6_BASE_URL;
if (!BASE_URL) {
  throw new Error(
    'K6_BASE_URL is required — set it to the staging environment URL. ' +
    'This script refuses to run against a default target (FR-043).',
  );
}

const allowedHosts = __ENV.LOADTEST_ALLOWED_HOSTS || '';
if (allowedHosts) {
  const parsed = new URL(BASE_URL);
  const host = parsed.hostname;
  const pattern = new RegExp(allowedHosts);
  if (!pattern.test(host)) {
    throw new Error(
      `Host "${host}" does not match LOADTEST_ALLOWED_HOSTS pattern "${allowedHosts}". ` +
      'Refusing to run (FR-043).',
    );
  }
}

export const options = {
  thresholds: {
    http_req_failed: ['rate<0.01'],
    http_req_duration: ['p(95)<500'],
  },
  stages: [
    { duration: '30s', target: 50 },
    { duration: '1m', target: 200 },
    { duration: '30s', target: 0 },
  ],
  noConnectionReuse: true,
};

function getAuthToken() {
  const url = `${BASE_URL}/api/v1/auth/login`;
  const payload = JSON.stringify({
    email: 'loadtest@example.com',
    password: 'loadtest-password',
  });
  const params = {
    headers: { 'Content-Type': 'application/json' },
  };
  const res = http.post(url, payload, params);
  if (res.status === 200) {
    const body = JSON.parse(res.body);
    return body.token || body.access_token || '';
  }
  console.warn(`Auth returned ${res.status}, continuing without token`);
  return '';
}

const dashboardEndpoints = [
  '/api/v1/conversations',
  '/api/v1/analytics/summary',
  '/api/v1/knowledge-base?page_size=20',
];

export default function () {
  const token = getAuthToken();
  const headers = {
    'Content-Type': 'application/json',
    Authorization: `Bearer ${token}`,
  };

  for (const endpoint of dashboardEndpoints) {
    const url = `${BASE_URL}${endpoint}`;
    const res = http.get(url, { headers });

    check(res, {
      'status is 2xx': (r) => r.status >= 200 && r.status < 300,
      'response time < 500ms': (r) => r.timings.duration < 500,
    });
  }

  sleep(1);
}
