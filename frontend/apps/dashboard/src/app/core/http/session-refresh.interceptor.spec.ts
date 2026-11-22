import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom, Observable } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { sessionRefreshInterceptor } from './session-refresh.interceptor';

/** The HTTP status a call failed with, or `0` when it succeeded. */
const failedWith = (call: Observable<unknown>): Promise<number> =>
  firstValueFrom(call).then(
    () => 0,
    (error: { status: number }) => error.status,
  );

describe('sessionRefreshInterceptor', () => {
  const UNAUTHENTICATED = {
    code: 'unauthenticated',
    detail: 'Authentication required',
    status: 401,
  };
  const REFUSED = { status: 401, statusText: 'Unauthorized' };
  const NO_CONTENT = { status: 204, statusText: 'No Content' };

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([sessionRefreshInterceptor])),
        provideHttpClientTesting(),
        { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1' } },
      ],
    });
  });

  const http = () => TestBed.inject(HttpTestingController);
  const client = () => TestBed.inject(HttpClient);

  afterEach(() => {
    http().verify();
  });

  it('refreshes once on a 401 and sends the call again', async () => {
    const result = firstValueFrom(client().get('/api/v1/tenant/customers'));

    http().expectOne('/api/v1/tenant/customers').flush(UNAUTHENTICATED, REFUSED);
    http().expectOne('/api/v1/auth/refresh').flush(null, NO_CONTENT);
    http().expectOne('/api/v1/tenant/customers').flush({ items: [] });

    expect(await result).toEqual({ items: [] });
  });

  it('makes concurrent calls wait for the same refresh', async () => {
    const a = firstValueFrom(client().get('/api/v1/tenant/a'));
    const b = firstValueFrom(client().get('/api/v1/tenant/b'));
    const c = firstValueFrom(client().get('/api/v1/tenant/c'));

    for (const path of ['a', 'b', 'c']) {
      http().expectOne(`/api/v1/tenant/${path}`).flush(UNAUTHENTICATED, REFUSED);
    }
    // Three calls failed, one refresh went out.
    http().expectOne('/api/v1/auth/refresh').flush(null, NO_CONTENT);
    for (const path of ['a', 'b', 'c']) {
      http().expectOne(`/api/v1/tenant/${path}`).flush({ ok: path });
    }

    expect(await Promise.all([a, b, c])).toEqual([{ ok: 'a' }, { ok: 'b' }, { ok: 'c' }]);
  });

  it('passes the original error on when the refresh fails', async () => {
    const result = failedWith(client().get('/api/v1/tenant/customers'));

    http().expectOne('/api/v1/tenant/customers').flush(UNAUTHENTICATED, REFUSED);
    http().expectOne('/api/v1/auth/refresh').flush(UNAUTHENTICATED, REFUSED);

    expect(await result).toBe(401);
  });

  it('retries a call only once', async () => {
    const result = failedWith(client().get('/api/v1/tenant/customers'));

    http().expectOne('/api/v1/tenant/customers').flush(UNAUTHENTICATED, REFUSED);
    http().expectOne('/api/v1/auth/refresh').flush(null, NO_CONTENT);
    // Refreshed, and still refused: no second refresh.
    http().expectOne('/api/v1/tenant/customers').flush(UNAUTHENTICATED, REFUSED);

    expect(await result).toBe(401);
  });

  it('leaves a step-up 401 to the step-up prompt', async () => {
    const result = failedWith(client().post('/api/v1/tenant/api-keys', {}));

    http()
      .expectOne('/api/v1/tenant/api-keys')
      .flush({ code: 'mfa_step_up_required', detail: 'Prove a code', status: 401 }, REFUSED);

    expect(await result).toBe(401);
    http().expectNone('/api/v1/auth/refresh');
  });

  it.each(['/api/v1/auth/login', '/api/v1/auth/signup', '/api/v1/auth/mfa/verify'])(
    'does not refresh for %s, which starts or ends a session',
    async (url) => {
      const result = failedWith(client().post(url, {}));

      http().expectOne(url).flush(UNAUTHENTICATED, REFUSED);

      expect(await result).toBe(401);
      http().expectNone('/api/v1/auth/refresh');
    },
  );

  it('refreshes before a sign-out whose access cookie has expired, so the session really ends', async () => {
    // The access cookie lasts ten minutes. A person who signs out later sends no cookie, gets a
    // 401, and the refresh brings the session back to life just long enough to end it.
    const result = firstValueFrom(client().post('/api/v1/auth/logout', {}));

    http().expectOne('/api/v1/auth/logout').flush(UNAUTHENTICATED, REFUSED);
    http().expectOne('/api/v1/auth/refresh').flush(null, NO_CONTENT);
    http().expectOne('/api/v1/auth/logout').flush(null, NO_CONTENT);

    expect(await result).toBeNull();
  });

  it('ignores a 401 from another origin and other statuses', async () => {
    const other = failedWith(client().get('https://other.example/api/v1/x'));
    http().expectOne('https://other.example/api/v1/x').flush(UNAUTHENTICATED, REFUSED);
    expect(await other).toBe(401);

    const forbidden = failedWith(client().get('/api/v1/tenant/x'));
    http()
      .expectOne('/api/v1/tenant/x')
      .flush({ code: 'unauthorized' }, { status: 403, statusText: 'Forbidden' });
    expect(await forbidden).toBe(403);

    http().expectNone('/api/v1/auth/refresh');
  });
});
