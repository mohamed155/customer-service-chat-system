import { marker } from '../i18n/marker';
import { inject, Injectable } from '@angular/core';
import { firstValueFrom } from 'rxjs';
import { ApiService } from '../api/api.service';
import type { components } from '../api/generated/api';

type Schemas = components['schemas'];

export type MfaStatus = Schemas['MfaStatusResponse'];
export type MfaEnrolment = Schemas['EnrolmentResponse'];
export type MfaRequired = Schemas['MfaRequiredResponse'];

/** What a person types for the second step: an authenticator code or a recovery code. */
export type MfaCredential = { readonly code: string } | { readonly recoveryCode: string };

/** The code of an error that means the user has to set up a second factor first. */
export const MFA_ENROLMENT_REQUIRED = 'mfa_enrolment_required';
/** The code of an error that means a sensitive action needs a code from the last five minutes. */
export const MFA_STEP_UP_REQUIRED = 'mfa_step_up_required';

/** The two-factor authentication endpoints (`/auth/mfa/*` and the reset routes). */
@Injectable({ providedIn: 'root' })
export class MfaService {
  private readonly api = inject(ApiService);

  async status(): Promise<MfaStatus> {
    return (await firstValueFrom(this.api.get<MfaStatus>('/auth/mfa/status'))).data;
  }

  /** Start setting up an authenticator: the secret and the `otpauth://` link, shown once. */
  async startEnrolment(password: string): Promise<MfaEnrolment> {
    return (await firstValueFrom(this.api.post<MfaEnrolment>('/auth/mfa/enrol', { password })))
      .data;
  }

  /** Prove the first code. Returns the recovery codes, shown once. */
  async confirmEnrolment(code: string): Promise<string[]> {
    const response = await firstValueFrom(
      this.api.post<Schemas['RecoveryCodesResponse']>('/auth/mfa/enrol/confirm', { code }),
    );
    return [...response.data.recoveryCodes];
  }

  /**
   * The second sign-in step. The session is the `app_session` cookie the answer sets; the answer's
   * body is not typed in the contract and nothing needs it (`AuthService` loads the user next).
   */
  async verify(credential: MfaCredential): Promise<void> {
    await firstValueFrom(this.api.post<unknown>('/auth/mfa/verify', credential));
  }

  /** Prove a code again for a sensitive action. */
  async stepUp(credential: MfaCredential): Promise<void> {
    await firstValueFrom(this.api.post<void>('/auth/mfa/step-up', credential));
  }

  /** Replace the recovery codes. Returns the new ones, shown once. */
  async regenerateRecoveryCodes(): Promise<string[]> {
    const response = await firstValueFrom(
      this.api.post<Schemas['RecoveryCodesResponse']>('/auth/mfa/recovery-codes', {}),
    );
    return [...response.data.recoveryCodes];
  }

  /** A tenant owner resets a member's two-factor authentication. */
  async resetMember(membershipId: string): Promise<void> {
    await firstValueFrom(this.api.post<void>(`/tenant/members/${membershipId}/mfa/reset`, {}));
  }

  /** A platform administrator resets a user's two-factor authentication. */
  async resetUser(userId: string): Promise<void> {
    await firstValueFrom(this.api.post<void>(`/platform/users/${userId}/mfa/reset`, {}));
  }
}

/** The catalog key of what a person sees for a failed code or password step. */
export function mfaErrorKey(status: number): string {
  if (status === 422) return marker('errors.codeInvalid');
  if (status === 429) return marker('errors.tooManyAttempts');
  if (status === 401) return marker('errors.signInExpired');
  return marker('errors.verificationFailed');
}
