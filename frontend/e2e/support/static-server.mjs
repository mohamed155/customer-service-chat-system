// Minimal static file server with SPA fallback for the mocked E2E suite.
// Usage: node e2e/support/static-server.mjs <root-dir> <port>
// Serving the production build (instead of `ng serve`) keeps page loads fast
// and deterministic when several Playwright workers run in parallel.
import { createReadStream, existsSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize, resolve } from 'node:path';

const root = resolve(process.argv[2] ?? 'dist/dashboard/browser');
const port = Number(process.argv[3] ?? 4201);
const indexFile = join(root, 'index.html');

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

createServer((req, res) => {
  const pathname = decodeURIComponent(new URL(req.url ?? '/', 'http://localhost').pathname);
  const candidate = normalize(join(root, pathname));
  const isFile =
    candidate.startsWith(root) && existsSync(candidate) && statSync(candidate).isFile();
  const file = isFile ? candidate : indexFile; // SPA fallback for client routes
  res.writeHead(200, {
    'content-type': TYPES[extname(file)] ?? 'application/octet-stream',
    'cache-control': 'no-store',
  });
  createReadStream(file).pipe(res);
}).listen(port, '127.0.0.1', () => {
  console.log(`Serving ${root} on http://127.0.0.1:${port}`);
});
