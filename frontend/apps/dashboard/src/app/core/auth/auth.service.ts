import { inject, Injectable, signal } from '@angular/core';
import { Router } from '@angular/router';
import { firstValueFrom } from 'rxjs';
import { ApiError } from '../api/api.models';
import { MeResponse } from '../api/tenant-api.models';
import { ApiService } from '../api/api.service';
import { mapHttpError, userMessageFor } from '../errors/http-error.mapper';
import { APP_PATHS } from '../router/app-paths';
import { CurrentUserService } from '../tenant/current-user.service';
import { TenantContextService } from '../tenant/tenant-context.service';

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

@Injectable({ providedIn: 'root' })
export class AuthService {
  private readonly api = inject(ApiService);
  private readonly currentUser = inject(CurrentUserService);
  private readonly tenantContext = inject(TenantContextService);
  private readonly router = inject(Router);
  private readonly pendingSignal = signal(false);

  readonly pending = this.pendingSignal.asReadonly();

  async login(email: string, password: string): Promise<void> {
    this.pendingSignal.set(true);

    try {
      await firstValueFrom(this.api.post<MeResponse>('/auth/login', { email, password }));
      await this.currentUser.load();
    } catch (error) {
      throw this.toLoginError(error);
    } finally {
      this.pendingSignal.set(false);
    }
  }

  async logout(): Promise<void> {
    this.pendingSignal.set(true);

    try {
      await firstValueFrom(this.api.post<void>('/auth/logout', {}));
    } catch (err) {
      console.error('Server logout failed, cleaning up locally', err);
    }

    this.currentUser.clear();
    this.tenantContext.clear();
    await this.router.navigate([`/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`]);
    this.pendingSignal.set(false);
  }

  private toLoginError(error: unknown): AuthLoginError {
    const apiError = isApiError(error) ? error : mapHttpError(error);
    return new AuthLoginError(
      apiError.status === 401 ? INVALID_CREDENTIALS_MESSAGE : userMessageFor(apiError),
      apiError,
    );
  }
}

const isApiError = (error: unknown): error is ApiError =>
  typeof error === 'object' &&
  error !== null &&
  typeof (error as ApiError).code === 'string' &&
  typeof (error as ApiError).message === 'string' &&
  typeof (error as ApiError).status === 'number';
