import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { MockStore, provideMockStore } from '@ngrx/store/testing';
import { firstValueFrom } from 'rxjs';
import { ApiError } from '../api/api.models';
import { APP_CONFIG, AppConfig } from '../config/app-config';
import { ApiErrorNotificationService } from '../errors/api-error-notification.service';
import { tenantContextActions } from '../state/tenant-context.feature';
import { CurrentUserService } from '../tenant/current-user.service';
import { TenantContextService } from '../tenant/tenant-context.service';
import { apiErrorInterceptor } from './api-error.interceptor';
import { authTokenInterceptor } from './auth-token.interceptor';

const config: AppConfig = {
  apiBaseUrl: '/api/v1',
  appName: 'Dashboard',
  environmentName: 'development',
  publicDashboardUrl: 'http://localhost:4200',
};

const configure = (initialState = {}) => {
  const router = { navigate: vi.fn().mockResolvedValue(true), url: '/tenant/overview' };
  const currentUser = { clear: vi.fn(), load: vi.fn().mockResolvedValue(undefined) };

  TestBed.configureTestingModule({
    providers: [
      provideHttpClient(withInterceptors([authTokenInterceptor, apiErrorInterceptor])),
      provideHttpClientTesting(),
      provideMockStore({ initialState }),
      { provide: APP_CONFIG, useValue: config },
      { provide: Router, useValue: router },
      { provide: CurrentUserService, useValue: currentUser },
    ],
  });

  return { router, currentUser };
};

describe('HTTP interceptors', () => {
  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('sets credentials for API requests and normalizes failures', async () => {
    configure();
    const requestPromise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/test')).catch(
      (error: ApiError) => error,
    );
    const request = TestBed.inject(HttpTestingController).expectOne('/api/v1/test');

    expect(request.request.headers.has('Authorization')).toBe(false);
    expect(request.request.withCredentials).toBe(true);

    request.flush(
      { error: { code: 'not_found', message: 'raw' } },
      { status: 404, statusText: 'Not Found' },
    );
    expect(await requestPromise).toMatchObject({ code: 'not_found', status: 404 });
  });

  it('publishes a user-facing message for tenant-scoped authorization failures', async () => {
    configure({
      tenantContext: {
        activeTenant: {
          id: 'tenant-1',
          name: 'Tenant 1',
          slug: 'tenant-1',
          status: 'active',
        },
        status: 'idle',
      },
    });
    const notifications = TestBed.inject(ApiErrorNotificationService);
    const requestPromise = firstValueFrom(
      TestBed.inject(HttpClient).get('/api/v1/tenant', {
        headers: { 'X-Tenant-ID': 'tenant-1' },
      }),
    ).catch((error: ApiError) => error);

    TestBed.inject(HttpTestingController)
      .expectOne('/api/v1/tenant')
      .flush(
        { error: { code: 'unauthorized', message: 'raw forbidden detail' } },
        { status: 403, statusText: 'Forbidden' },
      );

    expect(await requestPromise).toMatchObject({ code: 'unauthorized', status: 403 });
    expect(notifications.message()).toBe("You don't have access to this tenant.");
  });

  it.each(['platform_access_required', 'platform_access_expired'])(
    'clears workspace access and redirects once without signing out for %s',
    async (code) => {
      const { router, currentUser } = configure({
        tenantContext: {
          activeTenant: { id: 'tenant-1', name: 'Tenant 1' },
          status: 'idle',
        },
      });
      const notifications = TestBed.inject(ApiErrorNotificationService);
      const show = vi.spyOn(notifications, 'show');
      const dispatch = vi.spyOn(TestBed.inject(MockStore), 'dispatch');
      const client = TestBed.inject(HttpClient);
      const http = TestBed.inject(HttpTestingController);
      const promise = Promise.all(
        ['members', 'widgets'].map((resource) =>
          firstValueFrom(
            client.get(`/api/v1/tenant/${resource}`, {
              headers: { 'X-Tenant-ID': 'tenant-1' },
            }),
          ).catch((error: ApiError) => error),
        ),
      );

      for (const req of http.match((request) => request.url.startsWith('/api/v1/tenant/'))) {
        req.flush(
          { code, detail: 'Internal wording', status: 403 },
          { status: 403, statusText: 'Forbidden' },
        );
      }

      expect(await promise).toEqual([
        expect.objectContaining({ code }),
        expect.objectContaining({ code }),
      ]);
      expect(show).toHaveBeenCalledTimes(1);
      expect(dispatch).toHaveBeenCalledWith(tenantContextActions.clearActiveTenant());
      expect(currentUser.clear).not.toHaveBeenCalled();
      expect(router.navigate).toHaveBeenCalledExactlyOnceWith(['/platform', 'tenants']);
    },
  );

  it('does not end a newer access session for a refusal sent under an older one', async () => {
    const tenant = { id: 'tenant-1', name: 'Tenant 1' };
    const { router, currentUser } = configure({
      tenantContext: { activeTenant: tenant, status: 'idle' },
    });
    const notifications = TestBed.inject(ApiErrorNotificationService);
    const show = vi.spyOn(notifications, 'show');
    const dispatch = vi.spyOn(TestBed.inject(MockStore), 'dispatch');
    const service = TestBed.inject(TenantContextService);
    const client = TestBed.inject(HttpClient);
    const http = TestBed.inject(HttpTestingController);
    const session = (id: string) => ({
      id,
      tenantId: 'tenant-1',
      tenantName: 'Tenant 1',
      reason: 'Investigating a customer request',
      startedAt: new Date(Date.now() - 60_000).toISOString(),
      expiresAt: new Date(Date.now() + 30 * 60_000).toISOString(),
    });
    const restore = async (id: string) => {
      const restoring = service.restoreAccess(tenant as never);
      http.expectOne('/api/v1/platform/access-session').flush(session(id));
      await restoring;
    };

    await restore('access-old');
    const refused = firstValueFrom(
      client.get('/api/v1/tenant/members', { headers: { 'X-Tenant-ID': 'tenant-1' } }),
    ).catch((error: ApiError) => error);
    // Another tab starts new access for the same tenant before the old request is answered.
    await restore('access-new');
    http
      .expectOne('/api/v1/tenant/members')
      .flush(
        { code: 'platform_access_expired', detail: 'Internal wording', status: 403 },
        { status: 403, statusText: 'Forbidden' },
      );

    expect(await refused).toMatchObject({ code: 'platform_access_expired' });
    expect(service.accessSession()?.id).toBe('access-new');
    expect(dispatch).not.toHaveBeenCalledWith(tenantContextActions.clearActiveTenant());
    expect(show).not.toHaveBeenCalled();
    expect(currentUser.clear).not.toHaveBeenCalled();
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it('leaves the session alone when a password recovery request fails its origin proof', async () => {
    const { router, currentUser } = configure();
    const request = firstValueFrom(
      TestBed.inject(HttpClient).post('/api/v1/auth/password/reset', {}),
    ).catch((error: ApiError) => error);
    TestBed.inject(HttpTestingController)
      .expectOne('/api/v1/auth/password/reset')
      .flush(
        { error: { code: 'unauthorized', message: 'Origin not allowed' } },
        { status: 403, statusText: 'Forbidden' },
      );
    expect(await request).toMatchObject({ code: 'unauthorized', status: 403 });
    expect(currentUser.load).not.toHaveBeenCalled();
    expect(currentUser.clear).not.toHaveBeenCalled();
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it('clears client state and redirects when a non-login API request is unauthenticated', async () => {
    const { router, currentUser } = configure();
    const notifications = TestBed.inject(ApiErrorNotificationService);
    const requestPromise = firstValueFrom(
      TestBed.inject(HttpClient).get('/api/v1/conversations'),
    ).catch((error: ApiError) => error);

    TestBed.inject(HttpTestingController)
      .expectOne('/api/v1/conversations')
      .flush(
        { error: { code: 'unauthenticated', message: 'Authentication required' } },
        { status: 401, statusText: 'Unauthorized' },
      );

    expect(await requestPromise).toMatchObject({ code: 'unauthenticated', status: 401 });
    expect(currentUser.clear).toHaveBeenCalledTimes(1);
    expect(notifications.message()).toBe('Your session has expired. Sign in again to continue.');
    expect(router.navigate).toHaveBeenCalledWith(['/auth/login'], {
      queryParams: { returnUrl: window.location.pathname + window.location.search },
    });
  });

  it('does not redirect for login 401 responses', async () => {
    const { router, currentUser } = configure();
    const requestPromise = firstValueFrom(
      TestBed.inject(HttpClient).post('/api/v1/auth/login', {}),
    ).catch((error: ApiError) => error);

    TestBed.inject(HttpTestingController)
      .expectOne('/api/v1/auth/login')
      .flush(
        { error: { code: 'unauthenticated', message: 'Invalid email or password' } },
        { status: 401, statusText: 'Unauthorized' },
      );

    expect(await requestPromise).toMatchObject({ code: 'unauthenticated', status: 401 });
    expect(currentUser.clear).not.toHaveBeenCalled();
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it('does not redirect when anonymous app bootstrap receives a /me 401', async () => {
    const { router, currentUser } = configure();
    const requestPromise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/me')).catch(
      (error: ApiError) => error,
    );

    TestBed.inject(HttpTestingController)
      .expectOne('/api/v1/me')
      .flush(
        { error: { code: 'unauthenticated', message: 'Authentication required' } },
        { status: 401, statusText: 'Unauthorized' },
      );

    expect(await requestPromise).toMatchObject({ code: 'unauthenticated', status: 401 });
    expect(currentUser.clear).not.toHaveBeenCalled();
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it('triggers permission refresh on 403 unauthorized for non-/me requests', async () => {
    const { currentUser } = configure();
    const requestPromise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/tenant')).catch(
      (error: ApiError) => error,
    );

    TestBed.inject(HttpTestingController)
      .expectOne('/api/v1/tenant')
      .flush(
        { error: { code: 'unauthorized', message: 'Access denied' } },
        { status: 403, statusText: 'Forbidden' },
      );

    await requestPromise;
    expect(currentUser.load).toHaveBeenCalledTimes(1);
  });

  it('skips permission refresh on 403 for /me request (no refresh loop)', async () => {
    const { currentUser } = configure();
    const requestPromise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/me')).catch(
      (error: ApiError) => error,
    );

    TestBed.inject(HttpTestingController)
      .expectOne('/api/v1/me')
      .flush(
        { error: { code: 'unauthorized', message: 'Access denied' } },
        { status: 403, statusText: 'Forbidden' },
      );

    await requestPromise;
    expect(currentUser.load).not.toHaveBeenCalled();
  });

  it('sends the user to set up two-step verification on mfa_enrolment_required', async () => {
    const { router } = configure();
    const requestPromise = firstValueFrom(TestBed.inject(HttpClient).get('/api/v1/tenant')).catch(
      (error: ApiError) => error,
    );

    TestBed.inject(HttpTestingController)
      .expectOne('/api/v1/tenant')
      .flush(
        { error: { code: 'mfa_enrolment_required', message: 'Set up two-factor authentication' } },
        { status: 403, statusText: 'Forbidden' },
      );

    const error = (await requestPromise) as ApiError;
    expect(error.code).toBe('mfa_enrolment_required');
    expect(router.navigate).toHaveBeenCalledWith(['/mfa-setup'], {
      queryParams: { returnUrl: expect.any(String) },
    });
  });
  describe('paths that merely contain "me" are workspace calls (MOH-60)', () => {
    const tenantState = {
      tenantContext: {
        activeTenant: { id: 'tenant-1', name: 'Tenant 1', slug: 'tenant-1', status: 'active' },
        status: 'idle',
      },
    };

    it.each(['/api/v1/tenant/members', '/api/v1/tenant/conversations/c1/messages'])(
      'handles a 403 on %s: refreshes permissions, tells the user, clears a lost tenant',
      async (url) => {
        const { currentUser } = configure(tenantState);
        const dispatch = vi.spyOn(TestBed.inject(MockStore), 'dispatch');
        const notifications = TestBed.inject(ApiErrorNotificationService);
        const requestPromise = firstValueFrom(
          TestBed.inject(HttpClient).get(url, { headers: { 'X-Tenant-ID': 'tenant-1' } }),
        ).catch((error: ApiError) => error);

        TestBed.inject(HttpTestingController)
          .expectOne(url)
          .flush(
            { error: { code: 'unauthorized', message: 'raw' } },
            { status: 403, statusText: 'Forbidden' },
          );
        await requestPromise;

        expect(currentUser.load).toHaveBeenCalledTimes(1);
        expect(notifications.message()).toBe("You don't have access to this tenant.");
        expect(dispatch).toHaveBeenCalledWith(tenantContextActions.clearActiveTenant());
      },
    );

    it('treats an expired session on /tenant/members like any other API call', async () => {
      const { router, currentUser } = configure();
      const requestPromise = firstValueFrom(
        TestBed.inject(HttpClient).get('/api/v1/tenant/members'),
      ).catch((error: ApiError) => error);

      TestBed.inject(HttpTestingController)
        .expectOne('/api/v1/tenant/members')
        .flush(
          { error: { code: 'unauthenticated', message: 'raw' } },
          { status: 401, statusText: 'Unauthorized' },
        );
      await requestPromise;

      expect(currentUser.clear).toHaveBeenCalledTimes(1);
      expect(router.navigate).toHaveBeenCalled();
    });

    it('does not clear the tenant for a 403 on a platform route', async () => {
      configure(tenantState);
      const dispatch = vi.spyOn(TestBed.inject(MockStore), 'dispatch');
      const requestPromise = firstValueFrom(
        TestBed.inject(HttpClient).get('/api/v1/platform/tenants', {
          headers: { 'X-Tenant-ID': 'tenant-1' },
        }),
      ).catch((error: ApiError) => error);

      TestBed.inject(HttpTestingController)
        .expectOne('/api/v1/platform/tenants')
        .flush(
          { error: { code: 'unauthorized', message: 'raw' } },
          { status: 403, statusText: 'Forbidden' },
        );
      await requestPromise;

      expect(dispatch).not.toHaveBeenCalled();
    });
  });
  describe('request reference (MOH-64)', () => {
    it('hands the request ID of a failed call to the notification', async () => {
      configure({
        tenantContext: {
          activeTenant: { id: 'tenant-1', name: 'T', slug: 't', status: 'active' },
          status: 'idle',
        },
      });
      const notifications = TestBed.inject(ApiErrorNotificationService);
      const promise = firstValueFrom(
        TestBed.inject(HttpClient).get('/api/v1/tenant', {
          headers: { 'X-Tenant-ID': 'tenant-1' },
        }),
      ).catch((error: ApiError) => error);

      TestBed.inject(HttpTestingController)
        .expectOne('/api/v1/tenant')
        .flush(
          { error: { code: 'unauthorized', message: 'raw', request_id: 'req_0123' } },
          { status: 403, statusText: 'Forbidden' },
        );

      expect(await promise).toMatchObject({ requestId: 'req_0123' });
      expect(notifications.requestId()).toBe('req_0123');
    });

    it('keeps the ID the call was sent with when the network fails', async () => {
      configure();
      const promise = firstValueFrom(
        TestBed.inject(HttpClient).get('/api/v1/tenant/members', {
          headers: { 'X-Request-ID': 'req_sent' },
        }),
      ).catch((error: ApiError) => error);

      TestBed.inject(HttpTestingController)
        .expectOne('/api/v1/tenant/members')
        .error(new ProgressEvent('error'), { status: 0 });

      expect(await promise).toMatchObject({ code: 'network_error', requestId: 'req_sent' });
    });
  });
});
