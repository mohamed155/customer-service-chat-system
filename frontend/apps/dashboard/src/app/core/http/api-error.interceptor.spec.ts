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
import { apiErrorInterceptor } from './api-error.interceptor';
import { authTokenInterceptor } from './auth-token.interceptor';

const config: AppConfig = {
  apiBaseUrl: '/api/v1',
  appName: 'Dashboard',
  environmentName: 'development',
  enableNgRxDevtools: false,
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
});
