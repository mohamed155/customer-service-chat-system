import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { Subject, throwError } from 'rxjs';
import { ApiError, ApiResponse } from '../api/api.models';
import { MeResponse } from '../api/tenant-api.models';
import { ApiService } from '../api/api.service';
import { CurrentUserService } from '../tenant/current-user.service';
import { TenantContextService } from '../tenant/tenant-context.service';
import { AuthLoginError, AuthService, INVALID_CREDENTIALS_MESSAGE } from './auth.service';

describe('AuthService', () => {
  let service: AuthService;
  let api: { post: ReturnType<typeof vi.fn> };
  let currentUser: { clear: ReturnType<typeof vi.fn>; load: ReturnType<typeof vi.fn> };
  let tenantContext: { clear: ReturnType<typeof vi.fn> };
  let router: { navigate: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    api = { post: vi.fn() };
    currentUser = { clear: vi.fn(), load: vi.fn() };
    tenantContext = { clear: vi.fn() };
    router = { navigate: vi.fn().mockResolvedValue(true) };

    TestBed.configureTestingModule({
      providers: [
        AuthService,
        { provide: ApiService, useValue: api },
        { provide: CurrentUserService, useValue: currentUser },
        { provide: TenantContextService, useValue: tenantContext },
        { provide: Router, useValue: router },
      ],
    });

    service = TestBed.inject(AuthService);
  });

  it('posts credentials, exposes pending state, and reloads the current user', async () => {
    const response = new Subject<ApiResponse<MeResponse>>();
    currentUser.load.mockResolvedValue(undefined);
    api.post.mockReturnValue(response.asObservable());

    const login = service.login('agent@example.com', 'correct horse battery staple');

    expect(service.pending()).toBe(true);
    expect(api.post).toHaveBeenCalledWith('/auth/login', {
      email: 'agent@example.com',
      password: 'correct horse battery staple',
    });
    expect(currentUser.load).not.toHaveBeenCalled();

    response.next({ data: meResponse() });
    response.complete();
    await login;

    expect(currentUser.load).toHaveBeenCalledTimes(1);
    expect(service.pending()).toBe(false);
  });

  it('maps authentication failures to the generic invalid-credentials message', async () => {
    const apiError: ApiError = {
      code: 'unauthenticated',
      message: 'raw backend message',
      status: 401,
    };
    api.post.mockReturnValue(throwError(() => apiError));

    await expect(service.login('missing@example.com', 'wrong')).rejects.toMatchObject({
      message: INVALID_CREDENTIALS_MESSAGE,
      apiError,
    } satisfies Partial<AuthLoginError>);

    expect(currentUser.load).not.toHaveBeenCalled();
    expect(service.pending()).toBe(false);
  });

  it('maps non-authentication failures through the shared safe user-message mapper', async () => {
    const apiError: ApiError = {
      code: 'network_error',
      message: 'socket failed',
      status: 0,
    };
    api.post.mockReturnValue(throwError(() => apiError));

    await expect(service.login('agent@example.com', 'password')).rejects.toMatchObject({
      message: 'Check your connection and try again.',
      apiError,
    } satisfies Partial<AuthLoginError>);

    expect(service.pending()).toBe(false);
  });

  it('posts logout, clears user state, and navigates to login', async () => {
    const response = new Subject<ApiResponse<void>>();
    api.post.mockReturnValue(response.asObservable());

    const logout = service.logout();

    expect(service.pending()).toBe(true);
    expect(api.post).toHaveBeenCalledWith('/auth/logout', {});
    expect(currentUser.clear).not.toHaveBeenCalled();
    expect(tenantContext.clear).not.toHaveBeenCalled();
    expect(router.navigate).not.toHaveBeenCalled();

    response.next({ data: undefined });
    response.complete();
    await logout;

    expect(currentUser.clear).toHaveBeenCalledTimes(1);
    expect(tenantContext.clear).toHaveBeenCalledTimes(1);
    expect(router.navigate).toHaveBeenCalledWith(['/auth/login']);
    expect(service.pending()).toBe(false);
  });

  it('clears local state and navigates to login when server logout fails with 500', async () => {
    const apiError: ApiError = { code: 'internal_error', message: 'Server error', status: 500 };
    api.post.mockReturnValue(throwError(() => apiError));

    await service.logout();

    expect(currentUser.clear).toHaveBeenCalledTimes(1);
    expect(tenantContext.clear).toHaveBeenCalledTimes(1);
    expect(router.navigate).toHaveBeenCalledWith(['/auth/login']);
    expect(service.pending()).toBe(false);
  });

  it('clears local state and navigates to login when session is already expired (401)', async () => {
    const apiError: ApiError = { code: 'unauthenticated', message: 'Session expired', status: 401 };
    api.post.mockReturnValue(throwError(() => apiError));

    await service.logout();

    expect(currentUser.clear).toHaveBeenCalledTimes(1);
    expect(tenantContext.clear).toHaveBeenCalledTimes(1);
    expect(router.navigate).toHaveBeenCalledWith(['/auth/login']);
    expect(service.pending()).toBe(false);
  });
});

const meResponse = (): MeResponse => ({
  id: 'user-1',
  email: 'agent@example.com',
  displayName: 'Agent',
  platformRole: null,
  platformPermissions: [],
  staffTenantPermissions: null,
  memberships: [],
});
