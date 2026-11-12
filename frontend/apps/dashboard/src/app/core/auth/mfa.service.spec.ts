import { TestBed } from '@angular/core/testing';
import { of } from 'rxjs';
import { ApiService } from '../api/api.service';
import { MfaService, mfaErrorMessage } from './mfa.service';

describe('MfaService', () => {
  let service: MfaService;
  let api: { get: ReturnType<typeof vi.fn>; post: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    api = { get: vi.fn(), post: vi.fn() };
    TestBed.configureTestingModule({ providers: [{ provide: ApiService, useValue: api }] });
    service = TestBed.inject(MfaService);
  });

  it('reads the status', async () => {
    const status = {
      enrolled: true,
      recoveryCodesRemaining: 4,
      required: true,
      sessionSecondFactor: true,
    };
    api.get.mockReturnValue(of({ data: status }));
    await expect(service.status()).resolves.toEqual(status);
    expect(api.get).toHaveBeenCalledWith('/auth/mfa/status');
  });

  it('starts enrolment with the password and returns the secret and link', async () => {
    api.post.mockReturnValue(of({ data: { secret: 'S', otpauthUri: 'otpauth://totp/x' } }));
    await expect(service.startEnrolment('pw')).resolves.toEqual({
      secret: 'S',
      otpauthUri: 'otpauth://totp/x',
    });
    expect(api.post).toHaveBeenCalledWith('/auth/mfa/enrol', { password: 'pw' });
  });

  it('confirms enrolment and returns a copy of the recovery codes', async () => {
    api.post.mockReturnValue(of({ data: { recoveryCodes: ['A', 'B'] } }));
    await expect(service.confirmEnrolment('123456')).resolves.toEqual(['A', 'B']);
    expect(api.post).toHaveBeenCalledWith('/auth/mfa/enrol/confirm', { code: '123456' });
  });

  it('verifies, steps up and replaces recovery codes through their endpoints', async () => {
    api.post.mockReturnValue(of({ data: { recoveryCodes: ['N1'] } }));
    await service.verify({ recoveryCode: 'ABCDE-FGHJK' });
    expect(api.post).toHaveBeenLastCalledWith('/auth/mfa/verify', { recoveryCode: 'ABCDE-FGHJK' });
    await service.stepUp({ code: '123456' });
    expect(api.post).toHaveBeenLastCalledWith('/auth/mfa/step-up', { code: '123456' });
    await expect(service.regenerateRecoveryCodes()).resolves.toEqual(['N1']);
    expect(api.post).toHaveBeenLastCalledWith('/auth/mfa/recovery-codes', {});
  });

  it('resets a member by membership id and a user by user id', async () => {
    api.post.mockReturnValue(of({ data: null }));
    await service.resetMember('m-1');
    expect(api.post).toHaveBeenLastCalledWith('/tenant/members/m-1/mfa/reset', {});
    await service.resetUser('u-1');
    expect(api.post).toHaveBeenLastCalledWith('/platform/users/u-1/mfa/reset', {});
  });

  it('words the failures people can act on', () => {
    expect(mfaErrorMessage(422)).toBe('That code is not valid.');
    expect(mfaErrorMessage(429)).toBe('Too many attempts. Try again later.');
    expect(mfaErrorMessage(401)).toBe('Your sign-in expired. Sign in again.');
    expect(mfaErrorMessage(500)).toBe('Verification failed. Try again.');
  });
});
