import http from 'k6/http';
import { check, sleep } from 'k6';
import { URL } from 'https://jslib.k6.io/url/1.0.0/index.js';
import crypto from 'k6/crypto';

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

const WEBHOOK_SECRET = __ENV.LOADTEST_WEBHOOK_SECRET || 'loadtest-webhook-secret';
const CHANNEL = __ENV.LOADTEST_CHANNEL || 'whatsapp';

export const options = {
  thresholds: {
    http_req_failed: ['rate<0.01'],
    http_req_duration: ['p(95)<500'],
  },
  stages: [
    { duration: '30s', target: 20 },
    { duration: '1m', target: 80 },
    { duration: '30s', target: 0 },
  ],
  noConnectionReuse: true,
};

function signPayload(payload, secret) {
  const hmac = crypto.hmac('sha256', secret, payload, 'hex');
  return `sha256=${hmac}`;
}

function randomPhone() {
  const prefix = '+1555';
  const suffix = String(Math.floor(10000000 + Math.random() * 90000000));
  return prefix + suffix;
}

export default function () {
  const phone = randomPhone();
  const timestamp = Math.floor(Date.now() / 1000);

  const payload = JSON.stringify({
    object: CHANNEL,
    entry: [{
      id: `loadtest-${phone}`,
      changes: [{
        value: {
          messaging_product: CHANNEL,
          metadata: { phone_number_id: `loadtest-${randomPhone()}` },
          contacts: [{ profile: { name: 'Load Test User' }, wa_id: phone }],
          messages: [{
            from: phone,
            id: `loadtest-msg-${timestamp}-${Math.random().toString(36).substring(2, 8)}`,
            timestamp: String(timestamp),
            type: 'text',
            text: { body: `Load test message at ${timestamp}` },
          }],
        },
        field: 'messages',
      }],
    }],
  });

  const signature = signPayload(payload, WEBHOOK_SECRET);

  const res = http.post(
    `${BASE_URL}/hooks/v1/${CHANNEL}`,
    payload,
    {
      headers: {
        'Content-Type': 'application/json',
        'X-Hub-Signature-256': signature,
      },
    },
  );

  check(res, {
    'webhook accepted': (r) => r.status === 200,
    'response time < 500ms': (r) => r.timings.duration < 500,
  });

  sleep(1);
}
