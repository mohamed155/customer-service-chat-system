// The Content-Security-Policy the production edge sends with the dashboard document.
// `infra/Caddyfile` is the source of truth (a backend test keeps it equal to the application's
// default policy), so the mocked E2E suite reads it from there instead of keeping a copy.
// CommonJS so both the static server (ESM) and the Playwright specs can load it.
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');

const caddyfile = resolve(__dirname, '../../../infra/Caddyfile');

function dashboardCsp() {
  const text = readFileSync(caddyfile, 'utf8');
  // The dashboard document is the Caddy block that also denies framing; the widget blocks only
  // set `frame-ancestors *`.
  const match = text.match(
    /X-Frame-Options "DENY"\s+Content-Security-Policy "([^"]*default-src[^"]*)"/,
  );
  if (!match) {
    throw new Error(`No dashboard Content-Security-Policy found in ${caddyfile}`);
  }
  return match[1];
}

module.exports = { dashboardCsp };
