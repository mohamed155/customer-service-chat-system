import { inject, Injectable, signal } from '@angular/core';
import { Router } from '@angular/router';
import { firstValueFrom } from 'rxjs';
import { ApiError } from '../api/api.models';
import type { components } from '../api/generated/api';
import { SignupRequest } from '../api/tenant-api.models';
import { ApiService } from '../api/api.service';
import { mapHttpError, userMessageFor } from '../errors/http-error.mapper';
import { MfaCredential, MfaRequired, MfaService, mfaErrorMessage } from './mfa.service';
import { APP_PATHS } from '../router/app-paths';
import { CurrentUserService } from '../tenant/current-user.service';
import { TenantContextService } from '../tenant/tenant-context.service';

type Schemas = components['schemas'];

export const INVALID_CREDENTIALS_MESSAGE = 'Invalid email or password';

export class AuthLoginError extends Error {
  override readonly name = 'AuthLoginError';

  constructor(
    message: string,
    readonly apiError: ApiError,
  ) {
    super(message);
  }
}

/** What the password step led to. */
export type LoginOutcome = 'signed-in' | 'mfa-required';

export class AuthMfaError extends Error {
  override readonly name = 'AuthMfaError';

  constructor(
    message: string,
    readonly apiError: ApiError,
    /** The challenge is gone (expired or used): the user has to start again from the password. */
    readonly challengeExpired: boolean,
  ) {
    super(message);
  }
}

export class AuthSignupError extends Error {
  override readonly name = 'AuthSignupError';

  constructor(
    message: string,
    readonly apiError: ApiError,
  ) {
    super(message);
  }
}

@Injectable({ providedIn: 'root' })
export class AuthService {
  private readonly api = inject(ApiService);
  private readonly currentUser = inject(CurrentUserService);
  private readonly tenantContext = inject(TenantContextService);
  private readonly mfa = inject(MfaService);
  private readonly router = inject(Router);
  private readonly pendingSignal = signal(false);

  readonly pending = this.pendingSignal.asReadonly();

  /**
   * The password step. A user with two-factor authentication gets `mfa-required` and no session:
   * the second step ([`completeMfa`]) follows.
   */
  async login(email: string, password: string): Promise<LoginOutcome> {
    this.pendingSignal.set(true);

    try {
      const response = await firstValueFrom(
        this.api.post<Schemas['MeResponse'] | Schemas['MfaRequiredResponse']>('/auth/login', {
          email,
          password,
        } satisfies Schemas['LoginRequest']),
      );
      if (isMfaRequired(response.data)) {
        return 'mfa-required';
      }
      await this.currentUser.load();
      return 'signed-in';
    } catch (error) {
      throw this.toLoginError(error);
    } finally {
      this.pendingSignal.set(false);
    }
  }

  /** The second sign-in step: an authenticator code or a recovery code. */
  async completeMfa(credential: MfaCredential): Promise<void> {
    this.pendingSignal.set(true);

    try {
      await this.mfa.verify(credential);
      await this.currentUser.load();
    } catch (error) {
      throw this.toMfaError(error);
    } finally {
      this.pendingSignal.set(false);
    }
  }

  async signup(payload: SignupRequest): Promise<void> {
    this.pendingSignal.set(true);

    try {
      await firstValueFrom(this.api.post<Schemas['MeResponse']>('/auth/signup', payload));
      await this.currentUser.load();
    } catch (error) {
      throw this.toSignupError(error);
    } finally {
      this.pendingSignal.set(false);
    }
  }

  async logout(options: { returnUrl?: string } = {}): Promise<void> {
    this.pendingSignal.set(true);

    try {
      await firstValueFrom(this.api.post<void>('/auth/logout', {}));
    } catch (err) {
      console.error('Server logout failed, cleaning up locally', err);
    }

    this.currentUser.clear();
    this.tenantContext.clear();
    const loginRoute = [`/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`];
    if (options.returnUrl) {
      await this.router.navigate(loginRoute, { queryParams: { returnUrl: options.returnUrl } });
    } else {
      await this.router.navigate(loginRoute);
    }
    this.pendingSignal.set(false);
  }

  private toLoginError(error: unknown): AuthLoginError {
    const apiError = isApiError(error) ? error : mapHttpError(error);
    return new AuthLoginError(
      apiError.status === 401 ? INVALID_CREDENTIALS_MESSAGE : userMessageFor(apiError),
      apiError,
    );
  }

  private toMfaError(error: unknown): AuthMfaError {
    const apiError = isApiError(error) ? error : mapHttpError(error);
    return new AuthMfaError(mfaErrorMessage(apiError.status), apiError, apiError.status === 401);
  }

  private toSignupError(error: unknown): AuthSignupError {
    const apiError = isApiError(error) ? error : mapHttpError(error);
    const message =
      apiError.status === 409
        ? 'An account with this email already exists. Please sign in instead.'
        : userMessageFor(apiError);
    return new AuthSignupError(message, apiError);
  }
}

const isMfaRequired = (data: Schemas['MeResponse'] | MfaRequired): data is MfaRequired =>
  typeof data === 'object' && data !== null && (data as MfaRequired).mfaRequired === true;

const isApiError = (error: unknown): error is ApiError =>
  typeof error === 'object' &&
  error !== null &&
  typeof (error as ApiError).code === 'string' &&
  typeof (error as ApiError).message === 'string' &&
  typeof (error as ApiError).status === 'number';
