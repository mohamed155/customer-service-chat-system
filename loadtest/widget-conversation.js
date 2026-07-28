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
    { duration: '30s', target: 30 },
    { duration: '1m', target: 100 },
    { duration: '30s', target: 0 },
  ],
  noConnectionReuse: true,
};

function randomId() {
  return Math.random().toString(36).substring(2, 10);
}

export default function () {
  const sessionId = randomId();
  const customerId = `loadtest-customer-${randomId()}`;

  const sessionPayload = JSON.stringify({
    customer_id: customerId,
    source: 'widget',
    metadata: { load_test: true, session_id: sessionId },
  });
  const sessionRes = http.post(`${BASE_URL}/widget/v1/sessions`, sessionPayload, {
    headers: { 'Content-Type': 'application/json' },
  });

  check(sessionRes, {
    'session created': (r) => r.status === 201 || r.status === 200,
  });

  let sessionToken = '';
  if (sessionRes.status === 200 || sessionRes.status === 201) {
    const body = JSON.parse(sessionRes.body);
    sessionToken = body.token || body.session_token || body.id || '';
  }

  const messagePayload = JSON.stringify({
    session_id: sessionId,
    content: `Load test message at ${Date.now()}`,
    role: 'customer',
  });
  const msgHeaders = { 'Content-Type': 'application/json' };
  if (sessionToken) {
    msgHeaders['Authorization'] = `Bearer ${sessionToken}`;
  }
  const msgRes = http.post(`${BASE_URL}/widget/v1/messages`, messagePayload, {
    headers: msgHeaders,
  });

  check(msgRes, {
    'message sent': (r) => r.status === 201 || r.status === 200,
    'response time < 500ms': (r) => r.timings.duration < 500,
  });

  sleep(2);
}
