// Test for infra/nginx/spa.conf (MOH-167): the web server of the dashboard and widget images.
//
// Runs the real nginx image with that config in front of a small fake build, and checks the three
// rules the config exists for: client routes get index.html, a missing build file is a 404 (never
// index.html, so a page that predates a deploy can tell its lazy chunks are gone), and the cache
// headers (hashed files for a year, everything else revalidated).
//
//   node --test infra/nginx-spa.test.mjs
//
// Needs Docker. With REQUIRE_NGINX_TEST=1 (CI) a missing Docker fails the test instead of skipping it.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  chmodSync,
  mkdtempSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { createServer, request } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, describe, it } from "node:test";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const image = readFileSync(join(here, "Dockerfile.frontend"), "utf8").match(
  /^FROM\s+(nginx:\S+)/m,
)?.[1];

function dockerAvailable() {
  try {
    execFileSync("docker", ["info"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function get(port, path) {
  return new Promise((resolve, reject) => {
    request({ host: "127.0.0.1", port, path }, (res) => {
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () =>
        resolve({
          status: res.statusCode,
          headers: res.headers,
          text: Buffer.concat(chunks).toString(),
        }),
      );
    })
      .on("error", reject)
      .end();
  });
}

const dockerOk = dockerAvailable();
const required = process.env.REQUIRE_NGINX_TEST === "1";

describe(
  "infra/nginx/spa.conf",
  { skip: !dockerOk && !required ? "Docker is not available" : false },
  () => {
    const container = `nginx-spa-${process.pid}`;
    let port;

    before(async () => {
      assert.ok(dockerOk, "REQUIRE_NGINX_TEST=1 but Docker is not available");
      assert.ok(image, "infra/Dockerfile.frontend has no nginx image");
      const probe = createServer();
      port = await new Promise((resolve) =>
        probe.listen(0, "127.0.0.1", () => resolve(probe.address().port)),
      );
      await new Promise((resolve) => probe.close(resolve));

      const site = mkdtempSync(join(tmpdir(), "nginx-spa-"));
      writeFileSync(join(site, "index.html"), "<html>the app</html>");
      writeFileSync(join(site, "main-4TYQZ6FJ.js"), "export const main = 1;");
      writeFileSync(join(site, "styles-QWERTY12.css"), "body{}");
      writeFileSync(join(site, "theme-init.js"), "document.title = 'x';");
      // The nginx worker runs as another user than the one that owns this directory (on Linux the
      // bind mount keeps the host's permissions; mkdtemp makes it 0700).
      chmodSync(site, 0o755);
      for (const name of [
        "index.html",
        "main-4TYQZ6FJ.js",
        "styles-QWERTY12.css",
        "theme-init.js",
      ]) {
        chmodSync(join(site, name), 0o644);
      }
      execFileSync("docker", [
        "run",
        "-d",
        "--rm",
        "--name",
        container,
        "-p",
        `127.0.0.1:${port}:80`,
        "-v",
        `${join(here, "nginx", "spa.conf")}:/etc/nginx/conf.d/default.conf:ro`,
        "-v",
        `${site}:/usr/share/nginx/html:ro`,
        image,
      ]);
      for (let i = 0; i < 60; i += 1) {
        try {
          await get(port, "/");
          return;
        } catch {
          await new Promise((resolve) => setTimeout(resolve, 500));
        }
      }
      throw new Error(
        "nginx did not start: " +
          execFileSync("docker", ["logs", container]).toString(),
      );
    });

    after(() => {
      try {
        execFileSync("docker", ["rm", "-f", container], { stdio: "ignore" });
      } catch {
        // never started
      }
    });

    it("serves index.html for the root and for client routes, to be revalidated", async () => {
      for (const path of ["/", "/tenant/customers", "/invite/abc.def-ghi"]) {
        const res = await get(port, path);
        assert.equal(res.status, 200, path);
        assert.equal(res.text, "<html>the app</html>", path);
        assert.equal(res.headers["cache-control"], "no-cache", path);
      }
    });

    it("serves a present hashed file for a year, as immutable", async () => {
      const js = await get(port, "/main-4TYQZ6FJ.js");
      assert.equal(js.status, 200);
      assert.equal(js.text, "export const main = 1;");
      assert.match(js.headers["content-type"], /javascript/);
      assert.equal(
        js.headers["cache-control"],
        "public, max-age=31536000, immutable",
      );
      const css = await get(port, "/styles-QWERTY12.css");
      assert.equal(css.status, 200);
      assert.equal(
        css.headers["cache-control"],
        "public, max-age=31536000, immutable",
      );
    });

    it("answers a missing hashed chunk with 404, not index.html", async () => {
      for (const path of ["/chunk-ABCD1234.js", "/styles-OLDOLD12.css"]) {
        const res = await get(port, path);
        assert.equal(res.status, 404, path);
        assert.ok(!res.text.includes("the app"), `${path} must not be index.html`);
      }
    });

    it("answers any other missing file with 404 too, and revalidates the present ones", async () => {
      for (const path of ["/missing.js", "/missing.css", "/main.js.map", "/logo.svg"]) {
        const res = await get(port, path);
        assert.equal(res.status, 404, path);
        assert.ok(!res.text.includes("the app"), `${path} must not be index.html`);
      }
      const themeInit = await get(port, "/theme-init.js");
      assert.equal(themeInit.status, 200);
      assert.equal(themeInit.headers["cache-control"], "no-cache");
    });

    it("serves index.html itself without caching", async () => {
      const res = await get(port, "/index.html");
      assert.equal(res.status, 200);
      assert.equal(res.headers["cache-control"], "no-cache");
    });
  },
);
