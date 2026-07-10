import { TestBed } from '@angular/core/testing';
import { Router, UrlSegment } from '@angular/router';
import { APP_PATHS } from './app-paths';
import { authGuard } from './auth.guard';
import { CurrentUserService } from '../tenant/current-user.service';

describe('authGuard', () => {
  let currentUser: { currentUser: ReturnType<typeof vi.fn> };
  let router: { parseUrl: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    currentUser = { currentUser: vi.fn() };
    router = { parseUrl: vi.fn().mockReturnValue('redirect') };

    TestBed.configureTestingModule({
      providers: [
        { provide: CurrentUserService, useValue: currentUser },
        { provide: Router, useValue: router },
      ],
    });
  });

  it('allows authenticated users', () => {
    currentUser.currentUser.mockReturnValue({ id: 'user-1' });

    const result = TestBed.runInInjectionContext(() =>
      authGuard({ path: APP_PATHS.tenant.base } as never, [], {} as never),
    );

    expect(result).toBe(true);
    expect(router.parseUrl).not.toHaveBeenCalled();
  });

  it('redirects signed-out users to login with the attempted URL', () => {
    currentUser.currentUser.mockReturnValue(null);

    const result = TestBed.runInInjectionContext(() =>
      authGuard(
        { path: APP_PATHS.tenant.base } as never,
        [new UrlSegment(APP_PATHS.tenant.base, {}), new UrlSegment(APP_PATHS.tenant.overview, {})],
        {} as never,
      ),
    );

    expect(router.parseUrl).toHaveBeenCalledWith(
      `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}?returnUrl=%2Ftenant%2Foverview`,
    );
    expect(result).toEqual('redirect');
  });

  it('uses root as the attempted URL when no path segments are available', () => {
    currentUser.currentUser.mockReturnValue(null);

    TestBed.runInInjectionContext(() => authGuard({ path: '' } as never, [], {} as never));

    expect(router.parseUrl).toHaveBeenCalledWith(
      `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}?returnUrl=%2F`,
    );
  });
});
