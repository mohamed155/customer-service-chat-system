import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { of, throwError } from 'rxjs';
import { ApiService } from '../api/api.service';
import { CurrentUserService } from '../tenant/current-user.service';
import { TenantContextService } from '../tenant/tenant-context.service';
import { AuthMfaError, AuthService } from './auth.service';
import { MfaService } from './mfa.service';

describe('AuthService two-step sign-in', () => {
  let service: AuthService;
  let api: { post: ReturnType<typeof vi.fn> };
  let mfa: { verify: ReturnType<typeof vi.fn> };
  let currentUser: { clear: ReturnType<typeof vi.fn>; load: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    api = { post: vi.fn() };
    mfa = { verify: vi.fn() };
    currentUser = { clear: vi.fn(), load: vi.fn().mockResolvedValue(undefined) };
    TestBed.configureTestingModule({
      providers: [
        AuthService,
        { provide: ApiService, useValue: api },
        { provide: MfaService, useValue: mfa },
        { provide: CurrentUserService, useValue: currentUser },
        { provide: TenantContextService, useValue: { clear: vi.fn() } },
        { provide: Router, useValue: { navigate: vi.fn().mockResolvedValue(true) } },
      ],
    });
    service = TestBed.inject(AuthService);
  });

  it('answers mfa-required, and loads no user, when the password step returns a challenge', async () => {
    api.post.mockReturnValue(of({ data: { mfaRequired: true } }));
    await expect(service.login('a@b.test', 'pw')).resolves.toBe('mfa-required');
    expect(currentUser.load).not.toHaveBeenCalled();
    expect(service.pending()).toBe(false);
  });

  it('answers signed-in and loads the user when there is a session', async () => {
    api.post.mockReturnValue(of({ data: { id: 'u1', email: 'a@b.test' } }));
    await expect(service.login('a@b.test', 'pw')).resolves.toBe('signed-in');
    expect(currentUser.load).toHaveBeenCalledTimes(1);
  });

  it('completes the sign-in with a code and then loads the user', async () => {
    mfa.verify.mockResolvedValue({});
    await service.completeMfa({ code: '123456' });
    expect(mfa.verify).toHaveBeenCalledWith({ code: '123456' });
    expect(currentUser.load).toHaveBeenCalledTimes(1);
    expect(service.pending()).toBe(false);
  });

  it.each([
    [422, 'errors.codeInvalid', false],
    [429, 'errors.tooManyAttempts', false],
    [401, 'errors.signInExpired', true],
    [500, 'errors.verificationFailed', false],
  ])('maps a %s from the second step to a catalog key', async (status, message, expired) => {
    mfa.verify.mockRejectedValue({ code: 'x', message: 'raw', status });
    const error = await service.completeMfa({ code: '123456' }).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(AuthMfaError);
    expect((error as AuthMfaError).messageKey).toBe(message);
    expect((error as AuthMfaError).challengeExpired).toBe(expired);
    expect(currentUser.load).not.toHaveBeenCalled();
    expect(service.pending()).toBe(false);
  });

  it('keeps unrelated errors out of the way of the password step', async () => {
    api.post.mockReturnValue(throwError(() => ({ code: 'x', message: 'x', status: 401 })));
    await expect(service.login('a@b.test', 'bad')).rejects.toMatchObject({
      messageKey: 'auth.errors.invalidCredentials',
    });
  });
});
