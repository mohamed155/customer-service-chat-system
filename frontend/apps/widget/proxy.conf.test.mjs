// Tests the widget dev-server proxy (apps/widget/proxy.conf.cjs, MOH-51) end to end: it starts the
// real `ng serve widget` against a small local upstream and checks what the upstream receives and
// what the browser side gets back. Run it with `pnpm test:widget-proxy` from frontend/.
//
// It checks the three things that fail silently if someone edits the proxy:
//   - `/widget/v1/...` reaches the upstream as `/api/v1/widget/v1/...` (the backend's real path),
//   - the browser's `Origin` header arrives unchanged (the domain allowlist reads it), while `Host`
//     is the upstream's,
//   - an event stream's response headers reach the client before the upstream has sent any body.

import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import http from 'node:http';
import net from 'node:net';
import { after, before, describe, it } from 'node:test';
import { fileURLToPath } from 'node:url';

const FRONTEND = fileURLToPath(new URL('../..', import.meta.url));
const SSE_BODY_DELAY_MS = 2500;

function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

/** Records every request it gets. `/events` answers with SSE headers at once and its body later. */
function startUpstream(port, seen) {
  const server = http.createServer((req, res) => {
    seen.push({ url: req.url, origin: req.headers.origin, host: req.headers.host });
    if (req.url.includes('/events')) {
      res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
      res.flushHeaders();
      setTimeout(() => res.end(': late keep-alive\n\n'), SSE_BODY_DELAY_MS);
      return;
    }
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ data: { ok: true } }));
  });
  return new Promise((resolve) => server.listen(port, '127.0.0.1', () => resolve(server)));
}

async function waitUntilServing(url, child, output) {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`ng serve exited early:\n${output.join('')}`);
    try {
      if ((await fetch(url)).ok) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`the dev server did not come up:\n${output.join('')}`);
}

describe('widget dev-server proxy', () => {
  const seen = [];
  let upstream;
  let devServer;
  let base;
  let upstreamPort;

  before(async () => {
    upstreamPort = await freePort();
    const devPort = await freePort();
    upstream = await startUpstream(upstreamPort, seen);
    const output = [];
    devServer = spawn(
      'pnpm',
      ['exec', 'ng', 'serve', 'widget', '--port', String(devPort), '--host', '127.0.0.1'],
      {
        cwd: FRONTEND,
        env: { ...process.env, WIDGET_PROXY_TARGET: `http://127.0.0.1:${upstreamPort}` },
        stdio: ['ignore', 'pipe', 'pipe'],
      },
    );
    devServer.stdout.on('data', (d) => output.push(String(d)));
    devServer.stderr.on('data', (d) => output.push(String(d)));
    base = `http://127.0.0.1:${devPort}`;
    await waitUntilServing(`${base}/`, devServer, output);
  });

  after(async () => {
    devServer?.kill('SIGTERM');
    await new Promise((resolve) => (upstream ? upstream.close(resolve) : resolve()));
    upstream?.closeAllConnections?.();
  });

  it('forwards /widget/v1/... to /api/v1/widget/v1/... with the query string', async () => {
    seen.length = 0;
    const response = await fetch(`${base}/widget/v1/config?widgetId=wgt_abc`);
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { data: { ok: true } });
    assert.equal(seen.at(-1).url, '/api/v1/widget/v1/config?widgetId=wgt_abc');
  });

  it("passes the browser's Origin through unchanged and sends the upstream's own Host", async () => {
    seen.length = 0;
    await fetch(`${base}/widget/v1/config?widgetId=wgt_abc`, {
      headers: { Origin: 'http://localhost:4201' },
    });
    const request = seen.at(-1);
    assert.equal(request.origin, 'http://localhost:4201', 'the allowlist reads this header');
    assert.equal(request.host, `127.0.0.1:${upstreamPort}`);
  });

  it('does not forward anything outside /widget/v1', async () => {
    seen.length = 0;
    const response = await fetch(`${base}/api/v1/auth/login`, { method: 'POST' });
    void response;
    assert.deepEqual(seen, [], 'the upstream must not see a request for /api/v1/auth/login');
  });

  it('sends event-stream headers to the client before the upstream has sent a body', async () => {
    const startedAt = Date.now();
    const headers = await new Promise((resolve, reject) => {
      const req = http.get(`${base}/widget/v1/conversations/abc/events`, (res) => {
        resolve({ status: res.statusCode, type: res.headers['content-type'], at: Date.now() });
        res.destroy();
      });
      req.on('error', reject);
    });
    assert.equal(headers.status, 200);
    assert.equal(headers.type, 'text/event-stream');
    assert.ok(
      headers.at - startedAt < SSE_BODY_DELAY_MS - 1000,
      `headers took ${headers.at - startedAt} ms; the upstream's body only starts after ${SSE_BODY_DELAY_MS} ms`,
    );
  });
});
