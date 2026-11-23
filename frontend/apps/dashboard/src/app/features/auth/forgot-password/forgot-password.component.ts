import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { FormControl, FormGroup, ReactiveFormsModule, Validators } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { firstValueFrom } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { PasswordResetService } from '../../../core/auth/password-reset.service';
import { toApiError } from '../../../core/errors/api-error';
import { errorMessageKey } from '../../../core/errors/error-message';
import { marker } from '../../../core/i18n/marker';
import { APP_PATHS } from '../../../core/router/app-paths';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { AuthCardComponent } from '../auth-card/auth-card.component';

@Component({
  selector: 'app-forgot-password',
  providers: [provideTranslocoScope('auth')],
  imports: [
    AuthCardComponent,
    ErrorReferenceComponent,
    ReactiveFormsModule,
    RouterLink,
    TranslocoPipe,
  ],
  templateUrl: './forgot-password.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ForgotPasswordComponent {
  private readonly recovery = inject(PasswordResetService);
  protected readonly form = new FormGroup({
    email: new FormControl('', {
      nonNullable: true,
      validators: [Validators.required, Validators.email, Validators.pattern(/@[^@]+\.[^@]+$/)],
    }),
  });
  protected readonly pending = signal(false);
  protected readonly accepted = signal(false);
  protected readonly error = signal<ApiError | null>(null);
  protected readonly loginUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`;

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error, { 422: marker('auth.signup.errors.emailInvalid') });
  }

  protected async submit(): Promise<void> {
    if (this.pending() || this.accepted() || this.form.invalid) return;
    this.pending.set(true);
    this.error.set(null);
    try {
      await firstValueFrom(this.recovery.forgot(this.form.controls.email.value));
      this.accepted.set(true);
    } catch (error) {
      this.error.set(toApiError(error));
    } finally {
      this.pending.set(false);
    }
  }
}
