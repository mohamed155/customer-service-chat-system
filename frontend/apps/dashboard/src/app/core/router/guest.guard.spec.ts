import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { guestGuard } from './guest.guard';
import { CurrentUserService } from '../tenant/current-user.service';

describe('guestGuard', () => {
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

  it('allows signed-out users', () => {
    currentUser.currentUser.mockReturnValue(null);

    const result = TestBed.runInInjectionContext(() =>
      guestGuard({ path: 'auth' } as never, [], {} as never),
    );

    expect(result).toBe(true);
    expect(router.parseUrl).not.toHaveBeenCalled();
  });

  it('redirects authenticated users to the app root', () => {
    currentUser.currentUser.mockReturnValue({ id: 'user-1' });

    const result = TestBed.runInInjectionContext(() =>
      guestGuard({ path: 'auth' } as never, [], {} as never),
    );

    expect(router.parseUrl).toHaveBeenCalledWith('/');
    expect(result).toEqual('redirect');
  });
});
