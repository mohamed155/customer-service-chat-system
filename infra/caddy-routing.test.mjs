// Routing test for infra/Caddyfile (MOH-51).
//
// Runs the real Caddy image with the production site block, with the upstreams swapped for small
// recording servers, and checks where each public path ends up. The case that matters most: the
// widget's public API (`/widget/v1/*`) shares the `/widget` prefix with the static widget app, and
// must reach the backend as `/api/v1/widget/v1/*`, not the static server.
//
//   node --test infra/caddy-routing.test.mjs
//
// Needs Docker. With REQUIRE_CADDY_TEST=1 (CI) a missing Docker fails the test instead of skipping it.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createServer, request } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, describe, it } from "node:test";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const caddyfile = readFileSync(join(here, "Caddyfile"), "utf8");
const staging = readFileSync(join(here, "docker-compose.staging.yml"), "utf8");
const image = staging.match(/image:\s*(caddy:\S+)/)?.[1];

function dockerAvailable() {
  try {
    execFileSync("docker", ["info"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

/** A server that answers every request with a JSON description of what it received. */
function recorder(name) {
  const seen = [];
  const server = createServer((req, res) => {
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      seen.push({
        method: req.method,
        url: req.url,
        forwardedFor: req.headers["x-forwarded-for"],
        body: Buffer.concat(chunks).toString(),
      });
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ upstream: name }));
    });
  });
  return { server, seen, name };
}

function listen(server) {
  return new Promise((resolve) => {
    server.listen(0, "0.0.0.0", () => resolve(server.address().port));
  });
}

/** The production site block, served over plain HTTP with the three upstreams replaced. */
function testCaddyfile({ port, backend, widget, dashboard }) {
  const start = caddyfile.indexOf("# Staging/production");
  assert.notEqual(
    start,
    -1,
    'infra/Caddyfile has no "# Staging/production" block',
  );
  let block = caddyfile.slice(start);
  const swaps = [
    ["https://{$APP_DOMAIN} {", `http://:${port} {`],
    ["backend:8080", `host.docker.internal:${backend}`],
    ["widget:80", `host.docker.internal:${widget}`],
    ["dashboard:80", `host.docker.internal:${dashboard}`],
  ];
  for (const [from, to] of swaps) {
    assert.ok(
      block.includes(from),
      `infra/Caddyfile no longer contains "${from}"`,
    );
    block = block.split(from).join(to);
  }
  const withoutTls = block.replace(/\n\s*tls \{[^}]*\}\n/, "\n");
  assert.notEqual(
    withoutTls,
    block,
    "infra/Caddyfile no longer has a tls block to strip",
  );
  return `{\n    auto_https off\n}\n\n${withoutTls}`;
}

function get(port, path, { method = "GET", body } = {}) {
  return new Promise((resolve, reject) => {
    const req = request(
      {
        host: "127.0.0.1",
        port,
        path,
        method,
        headers: body ? { "content-type": "application/json" } : {},
      },
      (res) => {
        const chunks = [];
        res.on("data", (c) => chunks.push(c));
        res.on("end", () =>
          resolve({
            status: res.statusCode,
            text: Buffer.concat(chunks).toString(),
          }),
        );
      },
    );
    req.on("error", reject);
    req.end(body);
  });
}

const dockerOk = dockerAvailable();
const required = process.env.REQUIRE_CADDY_TEST === "1";

describe(
  "infra/Caddyfile routing",
  { skip: !dockerOk && !required ? "Docker is not available" : false },
  () => {
    const backend = recorder("backend");
    const widget = recorder("widget");
    const dashboard = recorder("dashboard");
    const container = `caddy-routing-${process.pid}`;
    let port;

    before(async () => {
      assert.ok(dockerOk, "REQUIRE_CADDY_TEST=1 but Docker is not available");
      assert.ok(image, "infra/docker-compose.staging.yml has no caddy image");
      const ports = await Promise.all(
        [backend, widget, dashboard].map((r) => listen(r.server)),
      );
      // A free port for Caddy: let the OS choose, then release it.
      const probe = createServer();
      port = await listen(probe);
      await new Promise((resolve) => probe.close(resolve));

      const dir = mkdtempSync(join(tmpdir(), "caddy-routing-"));
      const file = join(dir, "Caddyfile");
      writeFileSync(
        file,
        testCaddyfile({
          port,
          backend: ports[0],
          widget: ports[1],
          dashboard: ports[2],
        }),
      );
      execFileSync("docker", [
        "run",
        "-d",
        "--rm",
        "--name",
        container,
        "--add-host=host.docker.internal:host-gateway",
        "-p",
        `127.0.0.1:${port}:${port}`,
        "-v",
        `${file}:/etc/caddy/Caddyfile:ro`,
        image,
      ]);
      // Wait until Caddy answers.
      for (let i = 0; i < 60; i += 1) {
        try {
          await get(port, "/");
          return;
        } catch {
          await new Promise((resolve) => setTimeout(resolve, 500));
        }
      }
      throw new Error(
        "Caddy did not start: " +
          execFileSync("docker", ["logs", container]).toString(),
      );
    });

    after(() => {
      try {
        execFileSync("docker", ["rm", "-f", container], { stdio: "ignore" });
      } catch {
        // never started
      }
      for (const r of [backend, widget, dashboard]) r.server.close();
    });

    function reset() {
      for (const r of [backend, widget, dashboard]) r.seen.length = 0;
    }

    it("sends the widget API to the backend under /api/v1, keeping the query", async () => {
      reset();
      const res = await get(port, "/widget/v1/config?widgetId=wgt_abc");
      assert.equal(JSON.parse(res.text).upstream, "backend");
      assert.equal(backend.seen.length, 1);
      assert.equal(
        backend.seen[0].url,
        "/api/v1/widget/v1/config?widgetId=wgt_abc",
      );
      assert.ok(
        backend.seen[0].forwardedFor,
        "the backend must see X-Forwarded-For",
      );
      assert.equal(widget.seen.length, 0);
    });

    it("keeps the method and body of widget API calls", async () => {
      reset();
      await get(port, "/widget/v1/sessions", {
        method: "POST",
        body: '{"widgetId":"wgt_abc"}',
      });
      assert.equal(backend.seen[0].method, "POST");
      assert.equal(backend.seen[0].url, "/api/v1/widget/v1/sessions");
      assert.equal(backend.seen[0].body, '{"widgetId":"wgt_abc"}');
    });

    it("routes the bare /widget/v1 path to the backend too", async () => {
      reset();
      await get(port, "/widget/v1");
      assert.equal(backend.seen[0]?.url, "/api/v1/widget/v1");
    });

    it("still serves the static widget app from the widget container, prefix stripped", async () => {
      reset();
      const res = await get(port, "/widget/index.html");
      assert.equal(JSON.parse(res.text).upstream, "widget");
      assert.equal(widget.seen[0].url, "/index.html");
      assert.equal(backend.seen.length, 0);
    });

    it("does not treat a longer path such as /widget/v10 as the widget API", async () => {
      reset();
      await get(port, "/widget/v10/x");
      assert.equal(widget.seen[0]?.url, "/v10/x");
      assert.equal(backend.seen.length, 0);
    });

    it("sends /api to the backend unchanged, not to the dashboard", async () => {
      reset();
      await get(port, "/api/v1/me?x=1");
      assert.equal(backend.seen[0]?.url, "/api/v1/me?x=1");
      assert.equal(dashboard.seen.length, 0);
    });

    it("sends /ws to the backend, not to the dashboard", async () => {
      reset();
      await get(port, "/ws/events");
      assert.equal(backend.seen[0]?.url, "/ws/events");
      assert.equal(dashboard.seen.length, 0);
    });

    it("sends the dashboard routes to the dashboard", async () => {
      reset();
      await get(port, "/tenant/overview");
      assert.equal(dashboard.seen[0]?.url, "/tenant/overview");
      assert.equal(backend.seen.length, 0);
    });
  },
);
