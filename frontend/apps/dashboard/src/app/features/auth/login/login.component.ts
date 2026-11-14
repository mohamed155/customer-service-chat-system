import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { FormControl, FormGroup, ReactiveFormsModule, Validators } from '@angular/forms';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { AuthLoginError, AuthService } from '../../../core/auth/auth.service';
import { MfaService } from '../../../core/auth/mfa.service';
import { marker } from '../../../core/i18n/marker';
import { APP_PATHS } from '../../../core/router/app-paths';
import { AuthCardComponent } from '../auth-card/auth-card.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

@Component({
  selector: 'app-login',
  providers: [provideTranslocoScope('auth')],
  imports: [
    AuthCardComponent,
    ErrorReferenceComponent,
    ReactiveFormsModule,
    RouterLink,
    TranslocoPipe,
  ],
  templateUrl: './login.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LoginComponent {
  private readonly auth = inject(AuthService);
  private readonly mfa = inject(MfaService);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);

  protected readonly form = new FormGroup({
    email: new FormControl('', {
      nonNullable: true,
      validators: [Validators.required, Validators.email],
    }),
    password: new FormControl('', {
      nonNullable: true,
      validators: [Validators.required],
    }),
  });
  protected readonly pending = this.auth.pending;
  /** Catalog key of what went wrong, or `null`. */
  protected readonly errorKey = signal<string | null>(null);
  /** The request reference of the failed call, shown with the message (MOH-64). */
  protected readonly errorRequestId = signal<string | null>(null);
  protected readonly signupUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.signup}`;
  protected readonly forgotUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.forgotPassword}`;

  protected async submit(): Promise<void> {
    if (this.form.invalid) {
      this.form.markAllAsTouched();
      return;
    }

    this.errorKey.set(null);
    this.errorRequestId.set(null);
    const { email, password } = this.form.getRawValue();

    try {
      const outcome = await this.auth.login(email, password);
      const returnUrl = this.route.snapshot.queryParamMap.get('returnUrl');
      const queryParams = returnUrl ? { returnUrl } : {};
      if (outcome === 'mfa-required') {
        await this.router.navigate([`/${APP_PATHS.auth.base}/${APP_PATHS.auth.mfa}`], {
          queryParams,
        });
        return;
      }
      if (await this.needsSecondFactorSetup()) {
        await this.router.navigate([`/${APP_PATHS.mfaSetup}`], { queryParams });
        return;
      }
      await this.router.navigateByUrl(this.safeReturnUrl());
    } catch (error) {
      this.errorKey.set(
        error instanceof AuthLoginError ? error.messageKey : marker('auth.errors.signInFailed'),
      );
      this.errorRequestId.set(
        error instanceof AuthLoginError ? (error.apiError.requestId ?? null) : null,
      );
    }
  }

  /** The account has no second factor yet but this deployment requires one. */
  private async needsSecondFactorSetup(): Promise<boolean> {
    try {
      const status = await this.mfa.status();
      return status.required && !status.sessionSecondFactor;
    } catch {
      // The status is a convenience: if it cannot be read, the first call that needs a second
      // factor sends the user to set one up.
      return false;
    }
  }

  private safeReturnUrl(): string {
    const fallback = `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.overview}`;
    const returnUrl = this.route.snapshot.queryParamMap.get('returnUrl');
    if (!returnUrl || !returnUrl.startsWith('/') || returnUrl.startsWith('//')) return fallback;
    if (/^[a-z][a-z\d+.-]*:/i.test(returnUrl)) return fallback;
    return returnUrl;
  }
}
