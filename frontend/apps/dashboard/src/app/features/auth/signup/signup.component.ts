import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { FormControl, FormGroup, ReactiveFormsModule, Validators } from '@angular/forms';
import { Router, RouterLink } from '@angular/router';
import { ApiError } from '../../../core/api/api.models';
import { AuthService, AuthSignupError } from '../../../core/auth/auth.service';
import { fieldErrorKey } from '../../../core/errors/error-message';
import { marker } from '../../../core/i18n/marker';
import { APP_PATHS } from '../../../core/router/app-paths';
import { AuthCardComponent } from '../auth-card/auth-card.component';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

type SignupField = 'workspaceName' | 'displayName' | 'email' | 'password';

const API_FIELD_BY_NAME: Record<string, SignupField> = {
  workspaceName: 'workspaceName',
  displayName: 'displayName',
  email: 'email',
  password: 'password',
};

@Component({
  selector: 'app-signup',
  providers: [provideTranslocoScope('auth')],
  imports: [
    AuthCardComponent,
    ReactiveFormsModule,
    RouterLink,
    ButtonComponent,
    FormFieldComponent,
    InlineAlertComponent,
    TranslocoPipe,
  ],
  templateUrl: './signup.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SignupComponent {
  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);

  protected readonly form = new FormGroup({
    workspaceName: new FormControl('', {
      nonNullable: true,
      validators: [Validators.required, Validators.maxLength(200)],
    }),
    displayName: new FormControl('', {
      nonNullable: true,
      validators: [Validators.required, Validators.maxLength(200)],
    }),
    email: new FormControl('', {
      nonNullable: true,
      validators: [Validators.required, Validators.email],
    }),
    password: new FormControl('', {
      nonNullable: true,
      validators: [Validators.required, Validators.minLength(8)],
    }),
  });
  protected readonly pending = this.auth.pending;
  /** Catalog key of what went wrong with the whole form, or `null`. */
  protected readonly errorKey = signal<string | null>(null);
  protected readonly loginUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`;

  protected async submit(): Promise<void> {
    if (this.form.invalid) {
      this.form.markAllAsTouched();
      return;
    }

    this.errorKey.set(null);
    this.clearApiControlErrors();
    const { workspaceName, displayName, email, password } = this.form.getRawValue();

    try {
      await this.auth.signup({ workspaceName, displayName, email, password });
      await this.router.navigateByUrl(`/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.overview}`);
    } catch (error) {
      this.handleSignupError(error);
    }
  }

  /** The catalog key of what is wrong with a field, or `null`. */
  protected controlError(name: SignupField): string | null {
    const control = this.form.controls[name];
    if (!control.touched || !control.errors) return null;

    if (control.errors['required']) return REQUIRED_KEYS[name];
    if (control.errors['maxlength']) return TOO_LONG_KEYS[name];
    if (name === 'email' && control.errors['email'])
      return marker('auth.signup.errors.emailInvalid');
    if (name === 'password' && control.errors['minlength']) {
      return marker('auth.signup.errors.passwordShort');
    }
    // The key of an error the API reported for this field.
    if (control.errors['api']) return control.errors['api'] as string;

    return null;
  }

  private handleSignupError(error: unknown): void {
    if (!(error instanceof AuthSignupError)) {
      this.errorKey.set(marker('errors.generic'));
      return;
    }

    if (error.apiError.status === 422) {
      this.applyFieldErrors(error.apiError);
      this.errorKey.set(null);
      return;
    }

    this.errorKey.set(error.messageKey);
  }

  private applyFieldErrors(apiError: ApiError): void {
    for (const detail of apiError.details ?? []) {
      const field = detail.field ? API_FIELD_BY_NAME[detail.field] : undefined;
      if (!field) continue;

      const control = this.form.controls[field];
      control.setErrors({ ...(control.errors ?? {}), api: fieldErrorKey(detail.code) });
      control.markAsTouched();
    }
  }

  private clearApiControlErrors(): void {
    for (const control of Object.values(this.form.controls)) {
      if (!control.errors?.['api']) continue;
      const errors = { ...control.errors };
      delete errors['api'];
      control.setErrors(Object.keys(errors).length > 0 ? errors : null);
    }
  }
}

const REQUIRED_KEYS: Record<SignupField, string> = {
  workspaceName: marker('auth.signup.errors.workspaceNameRequired'),
  displayName: marker('auth.signup.errors.displayNameRequired'),
  email: marker('auth.signup.errors.emailRequired'),
  password: marker('auth.signup.errors.passwordRequired'),
};

const TOO_LONG_KEYS: Record<SignupField, string> = {
  workspaceName: marker('auth.signup.errors.workspaceNameTooLong'),
  displayName: marker('auth.signup.errors.displayNameTooLong'),
  email: marker('auth.signup.errors.emailTooLong'),
  password: marker('auth.signup.errors.passwordTooLong'),
};
