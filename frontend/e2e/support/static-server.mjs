// Minimal static file server with SPA fallback for the mocked E2E suite.
// Usage: node e2e/support/static-server.mjs <root-dir> <port>
// Serving the production build (instead of `ng serve`) keeps page loads fast
// and deterministic when several Playwright workers run in parallel.
import { createReadStream, existsSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize, resolve } from 'node:path';
import csp_ from './csp.cjs';

const root = resolve(process.argv[2] ?? 'dist/dashboard/browser');
const port = Number(process.argv[3] ?? 4201);
const indexFile = join(root, 'index.html');
// Serve the dashboard under the same enforced policy the production edge sends, so every mocked
// E2E spec also runs against it. The widget build is served without it (its own policy is just
// `frame-ancestors *`).
const csp = root.includes('widget') ? null : csp_.dashboardCsp();

if (!existsSync(indexFile)) {
  console.error(`No build at ${root}. Run \`pnpm exec ng build dashboard\` first.`);
  process.exit(1);
}

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.txt': 'text/plain; charset=utf-8',
};

// The same list as infra/nginx/spa.conf: a route can hold a dot, a file of these types cannot be one.
const ASSET_EXTENSION =
  /\.(?:js|mjs|css|map|json|txt|webmanifest|ico|png|jpe?g|gif|webp|svg|woff2?|ttf|otf)$/i;

createServer((req, res) => {
  const pathname = decodeURIComponent(new URL(req.url ?? '/', 'http://localhost').pathname);
  const candidate = normalize(join(root, pathname));
  const isFile =
    candidate.startsWith(root) && existsSync(candidate) && statSync(candidate).isFile();
  // A missing file with an extension (a build file a deploy removed) is a 404, as on the production
  // server (infra/nginx/spa.conf); index.html is only the fallback for client routes (MOH-167).
  if (!isFile && ASSET_EXTENSION.test(pathname)) {
    res.writeHead(404, {
      'content-type': 'text/plain; charset=utf-8',
      'cache-control': 'no-store',
    });
    res.end('Not found');
    return;
  }
  const file = isFile ? candidate : indexFile; // SPA fallback for client routes
  res.writeHead(200, {
    'content-type': TYPES[extname(file)] ?? 'application/octet-stream',
    'cache-control': 'no-store',
    ...(csp ? { 'content-security-policy': csp } : {}),
  });
  createReadStream(file).pipe(res);
}).listen(port, '127.0.0.1', () => {
  console.log(`Serving ${root} on http://127.0.0.1:${port}`);
});
