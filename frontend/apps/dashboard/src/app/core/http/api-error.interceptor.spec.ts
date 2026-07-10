import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { provideMockStore } from '@ngrx/store/testing';
import { firstValueFrom } from 'rxjs';
import { ApiError } from '../api/api.models';
import { APP_CONFIG, AppConfig } from '../config/app-config';
import { ApiErrorNotificationService } from '../errors/api-error-notification.service';
import { CurrentUserService } from '../tenant/current-user.service';
import { apiErrorInterceptor } from './api-error.interceptor';
import { authTokenInterceptor } from './auth-token.interceptor';

const config: AppConfig = {
  apiBaseUrl: '/api/v1',
  appName: 'Dashboard',
  environmentName: 'development',
  enableNgRxDevtools: false,
};

const configure = (initialState = {}) => {
  const router = { navigate: vi.fn().mockResolvedValue(true) };
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
});
