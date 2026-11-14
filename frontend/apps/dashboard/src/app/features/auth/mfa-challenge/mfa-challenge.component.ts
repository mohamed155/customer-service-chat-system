import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { FormControl, ReactiveFormsModule, Validators } from '@angular/forms';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { AuthMfaError, AuthService } from '../../../core/auth/auth.service';
import { marker } from '../../../core/i18n/marker';
import { APP_PATHS } from '../../../core/router/app-paths';
import { AuthCardComponent } from '../auth-card/auth-card.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

const CODE_PATTERN = /^\s*\d{6}\s*$/;
const RECOVERY_PATTERN = /^[A-Za-z0-9\s-]{10,16}$/;

/** The second sign-in step: a code from the authenticator app, or a recovery code. */
@Component({
  selector: 'app-mfa-challenge',
  providers: [provideTranslocoScope('auth')],
  imports: [
    AuthCardComponent,
    ErrorReferenceComponent,
    ReactiveFormsModule,
    RouterLink,
    TranslocoPipe,
  ],
  templateUrl: './mfa-challenge.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class MfaChallengeComponent {
  private readonly auth = inject(AuthService);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);

  protected readonly credential = new FormControl('', {
    nonNullable: true,
    validators: [Validators.required, Validators.pattern(CODE_PATTERN)],
  });
  protected readonly usingRecovery = signal(false);
  protected readonly pending = this.auth.pending;
  /** Catalog key of what went wrong, or `null`. */
  protected readonly errorKey = signal<string | null>(null);
  /** The request reference of the failed call, shown with the message (MOH-64). */
  protected readonly errorRequestId = signal<string | null>(null);
  protected readonly loginUrl = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`;

  protected toggleRecovery(): void {
    const recovery = !this.usingRecovery();
    this.usingRecovery.set(recovery);
    this.credential.setValidators([
      Validators.required,
      Validators.pattern(recovery ? RECOVERY_PATTERN : CODE_PATTERN),
    ]);
    this.credential.reset('');
    this.errorKey.set(null);
    this.errorRequestId.set(null);
  }

  protected async submit(): Promise<void> {
    if (this.credential.invalid) {
      this.credential.markAsTouched();
      return;
    }
    this.errorKey.set(null);
    this.errorRequestId.set(null);
    const value = this.credential.getRawValue().trim();
    try {
      await this.auth.completeMfa(this.usingRecovery() ? { recoveryCode: value } : { code: value });
      await this.router.navigateByUrl(this.safeReturnUrl());
    } catch (error) {
      if (error instanceof AuthMfaError) {
        this.errorKey.set(error.messageKey);
        this.errorRequestId.set(error.apiError.requestId ?? null);
        if (error.challengeExpired) {
          await this.router.navigate([this.loginUrl], {
            queryParams: this.route.snapshot.queryParamMap.has('returnUrl')
              ? { returnUrl: this.route.snapshot.queryParamMap.get('returnUrl') }
              : {},
          });
        }
      } else {
        this.errorKey.set(marker('errors.verificationFailed'));
      }
      this.credential.reset('');
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
