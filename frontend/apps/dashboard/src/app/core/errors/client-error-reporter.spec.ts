import { DOCUMENT } from '@angular/common';
import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { Store } from '@ngrx/store';
import { APP_CONFIG } from '../config/app-config';
import { CurrentUserService } from '../tenant/current-user.service';
import {
  ClientErrorReporter,
  DEDUPE_WINDOW_MS,
  MAX_BATCH,
  MAX_PER_MINUTE,
  MAX_PER_SESSION,
  PAUSE_AFTER_REFUSAL_MS,
  REPORT_FLUSH_MS,
} from './client-error-reporter';

const TENANT = '0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e0f';
const REQUEST_ID = 'req_0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e0f';

describe('ClientErrorReporter', () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let signedIn: boolean;
  let tenant: { id: string } | null;
  let environmentName: 'development' | 'production';
  let routeChain: string[];

  /** The routes the router has activated, outermost first, as a snapshot chain. */
  const snapshot = () => {
    let node: unknown = null;
    for (const path of [...routeChain].reverse()) {
      node = { routeConfig: { path }, firstChild: node };
    }
    return { root: { routeConfig: null, firstChild: node } };
  };

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-09T09:00:00Z'));
    signedIn = true;
    tenant = { id: TENANT };
    environmentName = 'production';
    routeChain = ['tenant', 'customers/:id'];
    fetchMock = vi.fn().mockResolvedValue({ status: 204 });
    vi.stubGlobal('fetch', fetchMock);
    document.head.innerHTML = '<script src="main-4TYQZ6FJ.js"></script>';
    localStorage.clear();
    TestBed.configureTestingModule({
      providers: [
        {
          provide: APP_CONFIG,
          useFactory: () => ({ apiBaseUrl: '/api/v1', environmentName }),
        },
        { provide: Store, useValue: { selectSignal: () => () => tenant } },
        {
          provide: Router,
          useValue: {
            routerState: {
              get snapshot() {
                return snapshot();
              },
            },
          },
        },
        {
          provide: CurrentUserService,
          useValue: { currentUser: () => (signedIn ? { id: 'user-1' } : null) },
        },
      ],
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    document.head.innerHTML = '';
  });

  const reporter = () => TestBed.inject(ClientErrorReporter);
  const sent = () =>
    fetchMock.mock.calls.flatMap(([, init]) => JSON.parse(init.body).reports as unknown[]) as {
      kind: string;
      message: string;
      route?: string;
      release?: string;
      request_id?: string;
      tenant_id?: string;
      count?: number;
    }[];
  const tick = (ms: number) => vi.advanceTimersByTimeAsync(ms);

  it('sends a scrubbed report after the flush interval, with keepalive and the CSRF header', async () => {
    reporter().report(
      new Error('Could not load https://app.test/x?token=abc for jane@example.com'),
    );

    expect(fetchMock).not.toHaveBeenCalled();
    await tick(REPORT_FLUSH_MS);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/v1/client-errors');
    expect(init).toMatchObject({
      method: 'POST',
      keepalive: true,
      credentials: 'include',
      headers: { 'Content-Type': 'application/json', 'X-Requested-With': 'XMLHttpRequest' },
    });
    expect(sent()).toHaveLength(1);
    expect(sent()[0]).toMatchObject({
      kind: 'error',
      message: 'Error: Could not load https://app.test/x for [email]',
      route: 'tenant/customers/:id',
      release: '4TYQZ6FJ',
      tenant_id: TENANT,
    });
  });

  it('carries the request ID of an ApiError, and describes it by its code only', async () => {
    reporter().report({
      code: 'internal_error',
      status: 500,
      message: 'Email jane@example.com is taken',
      requestId: REQUEST_ID,
    });
    await tick(REPORT_FLUSH_MS);

    expect(sent()[0]).toMatchObject({
      message: 'API error internal_error (status 500)',
      request_id: REQUEST_ID,
    });
    expect(JSON.stringify(sent())).not.toContain('jane');
  });

  it('leaves out an ID the backend would refuse, so one bad field cannot lose the batch', async () => {
    tenant = { id: 'tenant-1' };
    reporter().report({ code: 'x', status: 500, requestId: 'not-a-request-id' });
    await tick(REPORT_FLUSH_MS);

    const report = sent()[0];
    expect(report.tenant_id).toBeUndefined();
    expect(report.request_id).toBeUndefined();
  });

  it('sends no tenant when none is active, and no route before the first navigation', async () => {
    tenant = null;
    routeChain = [];
    reporter().report(new Error('x'));
    await tick(REPORT_FLUSH_MS);

    expect(sent()[0].tenant_id).toBeUndefined();
    expect(sent()[0].route).toBeUndefined();
  });

  it('reports a chunk-load failure as chunk_load, and the kind given wins', async () => {
    reporter().report(new TypeError('Failed to fetch dynamically imported module: /x.js'));
    reporter().report(new Error('other'), 'chunk_load');
    await tick(REPORT_FLUSH_MS);

    expect(sent().map((r) => r.kind)).toEqual(['chunk_load', 'chunk_load']);
  });

  it('reports a promise rejection nobody handled as unhandled_rejection', async () => {
    const service = reporter();
    service.start();
    const reason = new Error('rejected');
    const event = new Event('unhandledrejection') as PromiseRejectionEvent;
    Object.defineProperty(event, 'reason', { value: reason });
    window.dispatchEvent(event);

    service.report(reason); // what Angular's listener does with the reason
    service.report(new Error('thrown'));
    await tick(REPORT_FLUSH_MS);

    expect(sent().map((r) => r.kind)).toEqual(['unhandled_rejection', 'error']);
  });

  it('takes one error object in once, however many paths report it', async () => {
    const failure = new Error('twice');
    reporter().report(failure, 'chunk_load');
    reporter().report(failure);
    await tick(REPORT_FLUSH_MS);

    expect(sent()).toHaveLength(1);
    expect(sent()[0].count).toBeUndefined();
  });

  it('merges the same error into one report with a count', async () => {
    for (let i = 0; i < 5; i += 1) reporter().report(new Error('same'));
    await tick(REPORT_FLUSH_MS);

    expect(sent()).toHaveLength(1);
    expect(sent()[0].count).toBe(5);
  });

  it('does not send an error again within the dedupe window, but does after it', async () => {
    // One place in the code throwing the same error again: the same stack frame.
    const thrownAgain = () => new Error('again');
    reporter().report(thrownAgain());
    await tick(REPORT_FLUSH_MS);
    reporter().report(thrownAgain());
    await tick(REPORT_FLUSH_MS);
    expect(sent()).toHaveLength(1);

    await tick(DEDUPE_WINDOW_MS);
    reporter().report(thrownAgain());
    await tick(REPORT_FLUSH_MS);
    expect(sent()).toHaveLength(2);
  });

  it('sends nothing for a user who is not signed in', async () => {
    signedIn = false;
    reporter().report(new Error('x'));
    await tick(REPORT_FLUSH_MS * 2);

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('sends at once when a batch is full, with no more than a batch per request', async () => {
    for (let i = 0; i < MAX_BATCH; i += 1) reporter().report(new Error(`e${i}`));
    await tick(0);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(sent()).toHaveLength(MAX_BATCH);
  });

  it('flushes on demand and when the page is hidden or closing', async () => {
    const service = reporter();
    service.start();
    service.report(new Error('now'));
    service.flush();
    expect(fetchMock).toHaveBeenCalledTimes(1);

    service.report(new Error('hidden'));
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
    document.dispatchEvent(new Event('visibilitychange'));
    expect(fetchMock).toHaveBeenCalledTimes(2);

    service.report(new Error('closing'));
    window.dispatchEvent(new Event('pagehide'));
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it('sends nothing when there is nothing waiting', () => {
    reporter().flush();

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('stops at the per-minute budget and drops the rest', async () => {
    for (let round = 0; round < 3; round += 1) {
      for (let i = 0; i < MAX_BATCH; i += 1) reporter().report(new Error(`r${round}e${i}`));
      await tick(0);
    }

    expect(sent()).toHaveLength(MAX_PER_MINUTE);
  });

  it('stops at the budget of a page session', async () => {
    const perMinute = MAX_PER_MINUTE;
    let total = 0;
    for (let minute = 0; minute < 10; minute += 1) {
      for (let i = 0; i < perMinute; i += 1) {
        reporter().report(new Error(`m${minute}e${i}`));
        if ((i + 1) % MAX_BATCH === 0) await tick(0);
      }
      await tick(60_000);
      total = sent().length;
    }

    expect(total).toBe(MAX_PER_SESSION);
  });

  it.each([429, 401, 403])('pauses for a while after the server answers %i', async (status) => {
    fetchMock.mockResolvedValue({ status });
    reporter().report(new Error('first'));
    await tick(REPORT_FLUSH_MS);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    reporter().report(new Error('second'));
    await tick(REPORT_FLUSH_MS);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    await tick(PAUSE_AFTER_REFUSAL_MS);
    reporter().report(new Error('third'));
    await tick(REPORT_FLUSH_MS);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('does not throw or retry when the network fails', async () => {
    fetchMock.mockRejectedValue(new TypeError('Failed to fetch'));
    reporter().report(new Error('offline'));

    await tick(REPORT_FLUSH_MS * 3);

    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('adds the development identity header only in a development build', async () => {
    localStorage.setItem('app.devUserId', 'dev-user');
    reporter().report(new Error('prod'));
    await tick(REPORT_FLUSH_MS);
    expect(fetchMock.mock.calls[0][1].headers['X-Dev-User-Id']).toBeUndefined();

    TestBed.resetTestingModule();
    environmentName = 'development';
    TestBed.configureTestingModule({
      providers: [
        { provide: APP_CONFIG, useFactory: () => ({ apiBaseUrl: '/api/v1/', environmentName }) },
        { provide: Store, useValue: { selectSignal: () => () => tenant } },
        { provide: Router, useValue: { routerState: { snapshot: snapshot() } } },
        { provide: CurrentUserService, useValue: { currentUser: () => ({ id: 'u' }) } },
      ],
    });
    reporter().report(new Error('dev'));
    await tick(REPORT_FLUSH_MS);

    const [url, init] = fetchMock.mock.calls[1];
    expect(url).toBe('/api/v1/client-errors');
    expect(init.headers['X-Dev-User-Id']).toBe('dev-user');
  });

  it('reports the release as dev when the page has no main script hash', async () => {
    document.head.innerHTML = '<script src="theme-init.js"></script>';
    reporter().report(new Error('x'));
    await tick(REPORT_FLUSH_MS);

    expect(sent()[0].release).toBe('dev');
  });

  it('stops listening when destroyed', () => {
    const service = reporter();
    service.start();
    service.report(new Error('x'));
    TestBed.resetTestingModule();

    window.dispatchEvent(new Event('pagehide'));

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('takes DOCUMENT from the platform', () => {
    expect(TestBed.inject(DOCUMENT)).toBe(document);
  });
});
