import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { FormControl, FormGroup, ReactiveFormsModule, Validators } from '@angular/forms';
import { Router, RouterLink } from '@angular/router';
import { ApiError } from '../../../core/api/api.models';
import { AuthService, AuthSignupError } from '../../../core/auth/auth.service';
import { APP_PATHS } from '../../../core/router/app-paths';
import { AuthCardComponent } from '../auth-card/auth-card.component';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';

type SignupField = 'workspaceName' | 'displayName' | 'email' | 'password';

const API_FIELD_BY_NAME: Record<string, SignupField> = {
  workspaceName: 'workspaceName',
  displayName: 'displayName',
  email: 'email',
  password: 'password',
};

@Component({
  selector: 'app-signup',
  imports: [
    AuthCardComponent,
    ReactiveFormsModule,
    RouterLink,
    ButtonComponent,
    FormFieldComponent,
    InlineAlertComponent,
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
  protected readonly errorMessage = signal<string | null>(null);
  protected readonly loginUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`;

  protected async submit(): Promise<void> {
    if (this.form.invalid) {
      this.form.markAllAsTouched();
      return;
    }

    this.errorMessage.set(null);
    this.clearApiControlErrors();
    const { workspaceName, displayName, email, password } = this.form.getRawValue();

    try {
      await this.auth.signup({ workspaceName, displayName, email, password });
      await this.router.navigateByUrl(`/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.overview}`);
    } catch (error) {
      this.handleSignupError(error);
    }
  }

  protected controlError(name: SignupField): string | null {
    const control = this.form.controls[name];
    if (!control.touched || !control.errors) return null;

    if (control.errors['required']) return `${fieldLabel(name)} is required.`;
    if (control.errors['maxlength']) return `${fieldLabel(name)} must be 200 characters or fewer.`;
    if (name === 'email' && control.errors['email']) return 'Enter a valid email address.';
    if (name === 'password' && control.errors['minlength']) {
      return 'Password must be at least 8 characters.';
    }
    if (control.errors['api']) return control.errors['api'] as string;

    return null;
  }

  private handleSignupError(error: unknown): void {
    if (!(error instanceof AuthSignupError)) {
      this.errorMessage.set('Something went wrong. Please try again.');
      return;
    }

    if (error.apiError.status === 422) {
      this.applyFieldErrors(error.apiError);
      this.errorMessage.set(null);
      return;
    }

    this.errorMessage.set(error.message);
  }

  private applyFieldErrors(apiError: ApiError): void {
    for (const detail of apiError.details ?? []) {
      const field = detail.field ? API_FIELD_BY_NAME[detail.field] : undefined;
      if (!field) continue;

      const control = this.form.controls[field];
      control.setErrors({ ...(control.errors ?? {}), api: detail.message });
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

const fieldLabel = (name: SignupField): string => {
  switch (name) {
    case 'workspaceName':
      return 'Workspace name';
    case 'displayName':
      return 'Your name';
    case 'email':
      return 'Email';
    case 'password':
      return 'Password';
  }
};
