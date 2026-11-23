import { inject, Injectable } from '@angular/core';
import { ApiService } from '../api/api.service';
import type { components } from '../api/generated/api';

type Schemas = components['schemas'];

@Injectable({ providedIn: 'root' })
export class PasswordResetService {
  private readonly api = inject(ApiService);

  forgot(email: string) {
    return this.api.post<Schemas['ForgotPasswordResponse']>('/auth/password/forgot', {
      email,
    } satisfies Schemas['ForgotPasswordRequest']);
  }

  reset(token: string, password: string) {
    return this.api.post<void>('/auth/password/reset', {
      token,
      password,
    } satisfies Schemas['ResetPasswordRequest']);
  }
}
