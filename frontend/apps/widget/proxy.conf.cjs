// Dev-server proxy for `ng serve widget` (MOH-51).
//
// The widget calls the public API on the origin it is served from (`WIDGET_API_BASE` is
// `window.location.origin`) at `/widget/v1/...`, the base path of the public widget contract
// (specs/023-website-chat-widget/contracts/public-widget-api.md). The backend mounts that API under
// `/api/v1`, so this forwards `/widget/v1/...` to `/api/v1/widget/v1/...`. It also relays the
// server-sent events stream (`/widget/v1/conversations/{id}/events`) as it arrives.
//
// `changeOrigin` is off on purpose: the backend checks the widget's domain allowlist against the
// browser's `Origin` header (`http://localhost:4201`), and the proxy must pass it through unchanged.
//
// The backend is expected on port 8080 (the `PORT` default). Set `WIDGET_PROXY_TARGET` to use another
// address, for example `WIDGET_PROXY_TARGET=http://localhost:9090 pnpm exec ng serve widget --port 4201`.
const target = process.env.WIDGET_PROXY_TARGET || 'http://localhost:8080';

module.exports = {
  '/widget/v1': {
    target,
    secure: false,
    changeOrigin: false,
    pathRewrite: { '^/widget/v1': '/api/v1/widget/v1' },
    // Node holds a response's headers back until the first byte of the body is written. An event
    // stream can be quiet for a long time (the backend sends a keep-alive every 20 seconds), so
    // without this the browser sees no response, and the stream does not open, until then.
    // The proxy copies the upstream headers onto the response after this event, in the same tick, so
    // the flush waits for the next one.
    configure: (proxy) => {
      proxy.on('proxyRes', (proxyRes, _req, res) => {
        if (String(proxyRes.headers['content-type'] || '').startsWith('text/event-stream')) {
          process.nextTick(() => {
            if (!res.headersSent) res.flushHeaders();
          });
        }
      });
    },
  },
};
