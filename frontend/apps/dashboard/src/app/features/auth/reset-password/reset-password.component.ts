import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import {
  FormControl,
  FormGroup,
  ReactiveFormsModule,
  ValidatorFn,
  Validators,
} from '@angular/forms';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { distinctUntilChanged, firstValueFrom, map } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { PasswordResetService } from '../../../core/auth/password-reset.service';
import { toApiError } from '../../../core/errors/api-error';
import { errorMessageKey, fieldErrorKey } from '../../../core/errors/error-message';
import { marker } from '../../../core/i18n/marker';
import { APP_PATHS } from '../../../core/router/app-paths';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { AuthCardComponent } from '../auth-card/auth-card.component';

const passwordLength: ValidatorFn = (control) => {
  const password: string = control.value;
  // Use Unicode code points to match Rust's str::chars().
  const length = password.trim() ? Array.from(password).length : 0;
  if (length < 8) return { too_short: true };
  if (length > 128) return { too_long: true };
  return null;
};

@Component({
  selector: 'app-reset-password',
  providers: [provideTranslocoScope('auth')],
  imports: [
    AuthCardComponent,
    ErrorReferenceComponent,
    ReactiveFormsModule,
    RouterLink,
    TranslocoPipe,
  ],
  templateUrl: './reset-password.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ResetPasswordComponent {
  private readonly recovery = inject(PasswordResetService);
  private readonly currentUser = inject(CurrentUserService);
  private token = '';
  private routeVersion = 0;

  protected readonly form = new FormGroup(
    {
      password: new FormControl('', { nonNullable: true, validators: [passwordLength] }),
      confirm: new FormControl('', { nonNullable: true, validators: [Validators.required] }),
    },
    {
      validators: (group) =>
        group.get('password')?.value === group.get('confirm')?.value ? null : { mismatch: true },
    },
  );
  protected readonly pending = signal(false);
  protected readonly state = signal<'form' | 'expired' | 'success'>('expired');
  protected readonly error = signal<ApiError | null>(null);
  protected readonly errorKey = errorMessageKey;
  protected readonly loginUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`;
  protected readonly forgotUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.forgotPassword}`;

  constructor() {
    inject(ActivatedRoute)
      .paramMap.pipe(
        map((params) => params.get('token') ?? ''),
        distinctUntilChanged(),
        takeUntilDestroyed(),
      )
      .subscribe((token) => {
        this.routeVersion += 1;
        this.token = token;
        this.form.reset();
        this.error.set(null);
        this.pending.set(false);
        this.state.set(/^[A-Za-z0-9_-]{43}$/.test(token) ? 'form' : 'expired');
      });
  }

  protected passwordErrorKey(): string | null {
    const password = this.form.controls.password;
    if (!password.touched || !password.errors) return null;
    if (password.hasError('too_short')) return marker('auth.signup.errors.passwordShort');
    if (password.hasError('too_long')) return marker('auth.resetPassword.passwordLong');
    const serverCode = password.getError('server');
    return typeof serverCode === 'string' ? fieldErrorKey(serverCode) : null;
  }

  protected async submit(): Promise<void> {
    if (this.pending() || this.state() !== 'form') return;
    if (this.form.invalid) {
      this.form.markAllAsTouched();
      return;
    }

    this.pending.set(true);
    this.error.set(null);
    const { password } = this.form.getRawValue();
    const version = this.routeVersion;
    try {
      await firstValueFrom(this.recovery.reset(this.token, password));
      if (version !== this.routeVersion) return;
      this.form.reset();
      this.state.set('success');
      // Revalidate the displayed user if this tab had an active session when the link opened.
      if (this.currentUser.currentUser()) void this.currentUser.load().catch(() => undefined);
    } catch (error) {
      if (version !== this.routeVersion) return;
      const failure = toApiError(error);
      if (failure.code === 'reset_token_invalid') {
        this.form.reset();
        this.state.set('expired');
      } else {
        const field =
          failure.status === 422
            ? failure.details?.find((entry) => entry.field === 'password')
            : undefined;
        if (field) {
          this.form.controls.password.setErrors(
            field.code === 'too_short' || field.code === 'too_long'
              ? { [field.code]: true }
              : { server: field.code },
          );
          this.form.controls.password.markAsTouched();
        } else {
          this.error.set(failure);
        }
      }
    } finally {
      if (version === this.routeVersion) this.pending.set(false);
    }
  }
}
