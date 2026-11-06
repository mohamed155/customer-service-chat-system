import { Page, Route } from '@playwright/test';
import { existsSync, readFileSync } from 'fs';
import { extname, join, normalize, resolve } from 'path';

/**
 * Self-contained harness for the embeddable widget.
 *
 * Serves the built loader (dist/widget/widget.js) and widget app
 * (dist/widget/browser) from a fake widget origin, serves the host page from a
 * fake host origin, and mocks the public /widget/v1 API — so the specs need no
 * backend. Build first: `pnpm exec ng build widget && pnpm build:widget-loader`.
 */

export const WIDGET_ORIGIN = 'http://widget.e2e.test';
export const HOST_URL = 'http://host.e2e.test/';
export const WIDGET_ID = 'e2e-test-widget-id';

const DIST_DIR = resolve(process.cwd(), 'dist/widget');
const APP_DIR = join(DIST_DIR, 'browser');
const HOST_PAGE = resolve(process.cwd(), 'e2e/fixtures/widget-host.html');

const CONTENT_TYPES: Record<string, string> = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
};

interface WidgetMessage {
  id: string;
  sender: 'visitor' | 'assistant' | 'agent' | 'system';
  senderDisplayName?: string;
  body: string;
  createdAt: string;
}

interface WidgetConversation {
  id: string;
  handling: 'ai' | 'human' | 'closed';
  teamOnline: boolean;
  endedNote: boolean;
  messages: WidgetMessage[];
}

export interface WidgetHarnessOptions {
  /** Status returned for GET /widget/v1/config; 404 simulates an unknown widget ID. */
  readonly configStatus?: number;
  /** Seed an existing conversation, e.g. one already handed to a human. */
  readonly conversation?: Partial<WidgetConversation>;
}

function json(route: Route, body: unknown, status = 200) {
  return route.fulfill({
    status,
    contentType: 'application/json',
    body: JSON.stringify(body),
    headers: { 'access-control-allow-origin': '*' },
  });
}

function serveFile(route: Route, file: string) {
  return route.fulfill({
    status: 200,
    contentType: CONTENT_TYPES[extname(file)] ?? 'application/octet-stream',
    body: readFileSync(file),
  });
}

export async function installWidgetHarness(page: Page, options: WidgetHarnessOptions = {}) {
  if (!existsSync(join(DIST_DIR, 'widget.js')) || !existsSync(join(APP_DIR, 'index.html'))) {
    throw new Error(
      'Widget build missing. Run `pnpm exec ng build widget && pnpm build:widget-loader` first.',
    );
  }

  let conversation: WidgetConversation | null = options.conversation
    ? {
        id: 'wconv-e2e-1',
        handling: 'ai',
        teamOnline: true,
        endedNote: false,
        messages: [],
        ...options.conversation,
      }
    : null;
  let msgCounter = 0;
  // Server-sent events queued for the widget, and a hook that wakes up an
  // events request held open while the queue is empty (like a real stream).
  const pendingEvents: string[] = [];
  let wakeEventStream: (() => void) | null = null;

  await page.context().route(`${HOST_URL}**`, (route) => {
    const html = readFileSync(HOST_PAGE, 'utf8').replace(
      'src="/widget.js"',
      `src="${WIDGET_ORIGIN}/widget.js"`,
    );
    return route.fulfill({ status: 200, contentType: 'text/html', body: html });
  });

  await page.context().route(`${WIDGET_ORIGIN}/**`, async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();

    if (method === 'OPTIONS') {
      return route.fulfill({
        status: 204,
        headers: {
          'access-control-allow-origin': '*',
          'access-control-allow-headers': '*',
          'access-control-allow-methods': 'GET,POST,OPTIONS',
        },
      });
    }

    // ---- public widget API ----
    if (path === '/widget/v1/config') {
      if (options.configStatus && options.configStatus !== 200) {
        return json(
          route,
          { error: { code: 'not_found', message: 'Widget not found', details: [] } },
          options.configStatus,
        );
      }
      return json(route, {
        data: {
          widgetId: url.searchParams.get('widgetId') ?? WIDGET_ID,
          displayName: 'Support',
          primaryColor: '#4f46e5',
          welcomeMessage: 'Hi! How can we help?',
          position: 'bottom-right',
          theme: 'light',
          enabled: true,
        },
      });
    }
    if (path === '/widget/v1/sessions' && method === 'POST') {
      return json(route, {
        data: {
          sessionToken: 'e2e-session-token',
          expiresAt: new Date(Date.now() + 3_600_000).toISOString(),
        },
      });
    }
    if (path === '/widget/v1/conversation' && method === 'GET') {
      return json(route, { data: conversation ? { conversation } : null });
    }
    if (path === '/widget/v1/conversations' && method === 'POST') {
      conversation ??= {
        id: 'wconv-e2e-1',
        handling: 'ai',
        teamOnline: true,
        endedNote: false,
        messages: [],
      };
      return json(route, { data: { conversation } });
    }
    const msgMatch = path.match(/^\/widget\/v1\/conversations\/([^/]+)\/messages$/);
    if (msgMatch && method === 'POST') {
      const body = (request.postDataJSON() as { body: string }).body;
      const message: WidgetMessage = {
        id: `wmsg-${++msgCounter}`,
        sender: 'visitor',
        body,
        createdAt: new Date().toISOString(),
      };
      conversation?.messages.push(message);
      if (conversation?.handling === 'ai') {
        // Like the real backend, the assistant starts streaming a reply.
        pendingEvents.push(
          `event: ai.delta\ndata: ${JSON.stringify({ type: 'ai.delta', text: 'Let me check', messageId: null })}\n\n`,
        );
        wakeEventStream?.();
      }
      return json(route, { data: { message } });
    }
    if (/^\/widget\/v1\/conversations\/[^/]+\/events$/.test(path)) {
      if (pendingEvents.length === 0) {
        await new Promise<void>((resolveWait) => {
          wakeEventStream = resolveWait;
          setTimeout(resolveWait, 20_000);
        });
        wakeEventStream = null;
      }
      const body = pendingEvents.splice(0).join('') || ': keep-alive\n\n';
      // The page may already be closed when a held stream is released.
      return route
        .fulfill({ status: 200, contentType: 'text/event-stream', body })
        .catch(() => undefined);
    }
    if (path === '/widget/v1/feedback/pending') {
      return json(route, { data: null });
    }
    if (path.startsWith('/widget/v1/')) {
      return json(route, { data: null });
    }

    // ---- static assets: loader + widget app ----
    if (path === '/widget.js') {
      return serveFile(route, join(DIST_DIR, 'widget.js'));
    }
    const relative = path.replace(/^\/widget(?=\/|$)/, '').replace(/^\/+/, '');
    const candidate = normalize(join(APP_DIR, relative));
    if (relative && candidate.startsWith(APP_DIR) && existsSync(candidate)) {
      return serveFile(route, candidate);
    }
    return serveFile(route, join(APP_DIR, 'index.html'));
  });
}

/** Install the harness and open the host page that embeds the widget. */
export async function openWidgetHost(page: Page, options: WidgetHarnessOptions = {}) {
  await installWidgetHarness(page, options);
  await page.goto(HOST_URL);
}
