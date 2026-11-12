import {
  HttpClient,
  HttpContext,
  HttpErrorResponse,
  provideHttpClient,
  withInterceptors,
} from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { provideMockStore } from '@ngrx/store/testing';
import { TimeoutError } from 'rxjs';
import { ApiError } from '../api/api.models';
import { CurrentUserService } from '../tenant/current-user.service';
import { apiErrorInterceptor } from './api-error.interceptor';
import { requestIdInterceptor } from './request-id.interceptor';
import { APP_CONFIG, AppConfig } from '../config/app-config';
import {
  BACKOFF_BASE_MS,
  BACKOFF_CAP_MS,
  DEFAULT_TIMEOUT_MS,
  HTTP_RETRY,
  HTTP_TIMEOUT_MS,
  MAX_RETRIES,
  noTimeout,
  RETRY_RANDOM,
} from './http-resilience';
import { httpResilienceInterceptor } from './http-resilience.interceptor';

const config: AppConfig = {
  apiBaseUrl: '/api/v1',
  appName: 'Dashboard',
  environmentName: 'development',
  enableNgRxDevtools: false,
  publicDashboardUrl: 'http://localhost:4200',
};

const URL = '/api/v1/tenant/members';

describe('httpResilienceInterceptor', () => {
  let random = 0.999999;

  const setup = () => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([httpResilienceInterceptor])),
        provideHttpClientTesting(),
        { provide: APP_CONFIG, useValue: config },
        { provide: RETRY_RANDOM, useValue: () => random },
      ],
    });
    return {
      http: TestBed.inject(HttpClient),
      ctrl: TestBed.inject(HttpTestingController),
    };
  };

  beforeEach(() => {
    vi.useFakeTimers();
    random = 0.999999;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  /** Subscribes and records what comes out. */
  const run = (observable: ReturnType<HttpClient['get']>) => {
    const out: { value?: unknown; error?: unknown; done: boolean } = { done: false };
    observable.subscribe({
      next: (value) => (out.value = value),
      error: (error: unknown) => {
        out.error = error;
        out.done = true;
      },
      complete: () => (out.done = true),
    });
    return out;
  };

  describe('timeout', () => {
    it('fails with a TimeoutError when there is no answer within the default limit', () => {
      const { http, ctrl } = setup();
      const out = run(http.get(URL));
      const req = ctrl.expectOne(URL);

      vi.advanceTimersByTime(DEFAULT_TIMEOUT_MS - 1);
      expect(out.done).toBe(false);
      vi.advanceTimersByTime(1);

      expect(out.error).toBeInstanceOf(TimeoutError);
      expect(req.cancelled).toBe(true);
    });

    it('answers in time without a timeout', () => {
      const { http, ctrl } = setup();
      const out = run(http.get(URL));

      vi.advanceTimersByTime(DEFAULT_TIMEOUT_MS - 1);
      ctrl.expectOne(URL).flush({ ok: true });
      vi.advanceTimersByTime(DEFAULT_TIMEOUT_MS * 2);

      expect(out.value).toEqual({ ok: true });
      expect(out.error).toBeUndefined();
    });

    it('uses the limit a request sets in its context', () => {
      const { http, ctrl } = setup();
      const out = run(http.get(URL, { context: new HttpContext().set(HTTP_TIMEOUT_MS, 1000) }));
      ctrl.expectOne(URL);

      vi.advanceTimersByTime(1000);

      expect(out.error).toBeInstanceOf(TimeoutError);
    });

    it('has no limit for a request that opts out (an upload)', () => {
      const { http, ctrl } = setup();
      const out = run(http.post(URL, new FormData(), { context: noTimeout() }));
      const req = ctrl.expectOne(URL);

      vi.advanceTimersByTime(DEFAULT_TIMEOUT_MS * 10);
      expect(out.done).toBe(false);
      req.flush({ ok: true });

      expect(out.value).toEqual({ ok: true });
    });

    it('does not retry a timeout', () => {
      const { http, ctrl } = setup();
      const out = run(http.get(URL));
      ctrl.expectOne(URL);

      vi.advanceTimersByTime(DEFAULT_TIMEOUT_MS);
      vi.advanceTimersByTime(60_000);

      expect(out.error).toBeInstanceOf(TimeoutError);
      ctrl.verify();
    });
  });

  describe('retry', () => {
    it.each([502, 503, 504])('retries a GET after a %s and returns the later answer', (status) => {
      const { http, ctrl } = setup();
      const out = run(http.get(URL));

      ctrl.expectOne(URL).flush('gateway', { status, statusText: 'x' });
      vi.advanceTimersByTime(BACKOFF_BASE_MS);
      ctrl.expectOne(URL).flush({ ok: true });

      expect(out.value).toEqual({ ok: true });
    });

    it('retries after a dropped connection', () => {
      const { http, ctrl } = setup();
      const out = run(http.get(URL));

      ctrl.expectOne(URL).error(new ProgressEvent('error'), { status: 0 });
      vi.advanceTimersByTime(BACKOFF_BASE_MS);
      ctrl.expectOne(URL).flush({ ok: true });

      expect(out.value).toEqual({ ok: true });
    });

    it('retries a HEAD request too', () => {
      const { http, ctrl } = setup();
      const out = run(http.head(URL));

      ctrl.expectOne(URL).flush('x', { status: 503, statusText: 'x' });
      vi.advanceTimersByTime(BACKOFF_BASE_MS);
      ctrl.expectOne(URL).flush(null);

      expect(out.done).toBe(true);
      expect(out.error).toBeUndefined();
    });

    it(`gives up after ${MAX_RETRIES} retries and returns the last error`, () => {
      const { http, ctrl } = setup();
      const out = run(http.get(URL));

      for (let i = 0; i <= MAX_RETRIES; i++) {
        ctrl.expectOne(URL).flush('down', { status: 503, statusText: 'Unavailable' });
        vi.advanceTimersByTime(BACKOFF_CAP_MS + 1); // past any backoff, short of the next timeout
      }

      expect((out.error as HttpErrorResponse).status).toBe(503);
      ctrl.verify(); // no fourth request
    });

    it.each([400, 401, 403, 404, 409, 422, 500])('does not retry a GET after a %s', (status) => {
      const { http, ctrl } = setup();
      const out = run(http.get(URL));

      ctrl.expectOne(URL).flush('no', { status, statusText: 'x' });
      vi.advanceTimersByTime(60_000);

      expect((out.error as HttpErrorResponse).status).toBe(status);
      ctrl.verify();
    });

    it.each(['POST', 'PUT', 'PATCH', 'DELETE'])('does not retry a %s after a 503', (method) => {
      const { http, ctrl } = setup();
      const out = run(http.request(method, URL, { body: {} }));

      ctrl.expectOne(URL).flush('down', { status: 503, statusText: 'x' });
      vi.advanceTimersByTime(60_000);

      expect((out.error as HttpErrorResponse).status).toBe(503);
      ctrl.verify();
    });

    it('sends a GET once when the request opts out of retries', () => {
      const { http, ctrl } = setup();
      const out = run(http.get(URL, { context: new HttpContext().set(HTTP_RETRY, false) }));

      ctrl.expectOne(URL).flush('down', { status: 503, statusText: 'x' });
      vi.advanceTimersByTime(60_000);

      expect((out.error as HttpErrorResponse).status).toBe(503);
      ctrl.verify();
    });

    it('waits inside the jitter bounds: no wait at all at the low end, one step short at the top', () => {
      const { http, ctrl } = setup();

      random = 0; // the shortest wait: none
      run(http.get(URL));
      ctrl.expectOne(URL).flush('down', { status: 503, statusText: 'x' });
      vi.advanceTimersByTime(0);
      ctrl.expectOne(URL).flush({ ok: true });

      random = 0.999999; // the longest wait: one millisecond short of the first step
      run(http.get(URL));
      ctrl.expectOne(URL).flush('down', { status: 503, statusText: 'x' });
      vi.advanceTimersByTime(BACKOFF_BASE_MS - 2);
      ctrl.expectNone(URL);
      vi.advanceTimersByTime(1);
      ctrl.expectOne(URL).flush({ ok: true });
    });

    it('waits for Retry-After on a 503, not for the backoff', () => {
      const { http, ctrl } = setup();
      const out = run(http.get(URL));

      ctrl
        .expectOne(URL)
        .flush('down', { status: 503, statusText: 'x', headers: { 'Retry-After': '3' } });
      vi.advanceTimersByTime(2999);
      ctrl.expectNone(URL);
      vi.advanceTimersByTime(1);
      ctrl.expectOne(URL).flush({ ok: true });

      expect(out.value).toEqual({ ok: true });
    });

    it('retries a 429 that says when, and shows one that does not', () => {
      const { http, ctrl } = setup();
      const out = run(http.get(URL));
      ctrl
        .expectOne(URL)
        .flush('slow', { status: 429, statusText: 'x', headers: { 'Retry-After': '1' } });
      vi.advanceTimersByTime(1000);
      ctrl.expectOne(URL).flush({ ok: true });
      expect(out.value).toEqual({ ok: true });

      const bare = run(http.get(URL));
      ctrl.expectOne(URL).flush('slow', { status: 429, statusText: 'x' });
      vi.advanceTimersByTime(60_000);
      expect((bare.error as HttpErrorResponse).status).toBe(429);
      ctrl.verify();
    });
  });

  it('leaves a call to another server alone: no limit, no retry', () => {
    const { http, ctrl } = setup();
    const out = run(http.get('https://third-party.test/data'));
    const req = ctrl.expectOne('https://third-party.test/data');

    vi.advanceTimersByTime(DEFAULT_TIMEOUT_MS * 2);
    expect(out.done).toBe(false);
    req.flush('down', { status: 503, statusText: 'x' });
    vi.advanceTimersByTime(60_000);

    expect((out.error as HttpErrorResponse).status).toBe(503);
    ctrl.verify();
  });
});

describe('with the error interceptor outside it (the order in app.config)', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('turns a timeout into a typed ApiError that carries the request ID', () => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(
          withInterceptors([requestIdInterceptor, apiErrorInterceptor, httpResilienceInterceptor]),
        ),
        provideHttpClientTesting(),
        provideMockStore({ initialState: {} }),
        { provide: APP_CONFIG, useValue: config },
        { provide: Router, useValue: { navigate: vi.fn(), url: '/' } },
        { provide: CurrentUserService, useValue: { clear: vi.fn(), load: vi.fn() } },
      ],
    });
    let error: ApiError | undefined;
    TestBed.inject(HttpClient)
      .get(URL)
      .subscribe({ error: (e: ApiError) => (error = e) });
    const req = TestBed.inject(HttpTestingController).expectOne(URL);

    vi.advanceTimersByTime(DEFAULT_TIMEOUT_MS);

    expect(error).toMatchObject({
      code: 'timeout',
      status: 0,
      requestId: req.request.headers.get('X-Request-ID'),
    });
  });
});
