import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { FormControl, ReactiveFormsModule, Validators } from '@angular/forms';
import { DomSanitizer, SafeUrl } from '@angular/platform-browser';
import { ActivatedRoute, Router } from '@angular/router';
import { ApiError } from '../../../core/api/api.models';
import { MfaService, mfaErrorMessage } from '../../../core/auth/mfa.service';
import { mapHttpError } from '../../../core/errors/http-error.mapper';
import { APP_PATHS } from '../../../core/router/app-paths';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { copyToClipboard } from '../../../shared/utils/clipboard';
import { AuthCardComponent } from '../auth-card/auth-card.component';

type Step = 'password' | 'code' | 'recovery';

/**
 * Setting up an authenticator app: the current password, then the setup key and link, then a first
 * code, then the recovery codes (shown once). Reached by a signed-in user whose account needs a
 * second factor; the server sends them here with `mfa_enrolment_required`.
 */
@Component({
  selector: 'app-mfa-setup',
  imports: [AuthCardComponent, ReactiveFormsModule],
  templateUrl: './mfa-setup.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class MfaSetupComponent {
  private readonly mfa = inject(MfaService);
  private readonly currentUser = inject(CurrentUserService);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly sanitizer = inject(DomSanitizer);

  protected readonly step = signal<Step>('password');
  protected readonly password = new FormControl('', {
    nonNullable: true,
    validators: [Validators.required],
  });
  protected readonly code = new FormControl('', {
    nonNullable: true,
    validators: [Validators.required, Validators.pattern(/^\s*\d{6}\s*$/)],
  });
  protected readonly pending = signal(false);
  protected readonly errorMessage = signal<string | null>(null);
  protected readonly setupKey = signal('');
  protected readonly setupLink = signal<SafeUrl | null>(null);
  protected readonly recoveryCodes = signal<string[]>([]);
  protected readonly saved = signal(false);
  protected readonly copied = signal(false);

  protected async start(): Promise<void> {
    if (this.password.invalid) {
      this.password.markAsTouched();
      return;
    }
    await this.run(async () => {
      const enrolment = await this.mfa.startEnrolment(this.password.getRawValue());
      this.setupKey.set(enrolment.secret);
      // Angular treats an `otpauth:` link as unsafe. This one comes from our own server and is
      // only trusted when it has the one shape an authenticator link has. It is a URL for an
      // `href`, not HTML or script, so it is not a Trusted Types sink (MOH-170).
      this.setupLink.set(
        enrolment.otpauthUri.startsWith('otpauth://totp/')
          ? // eslint-disable-next-line html-safety/unsafe-html -- `otpauth://totp/` only, checked on this line; used as an `href`, never as HTML
            this.sanitizer.bypassSecurityTrustUrl(enrolment.otpauthUri)
          : null,
      );
      this.password.reset('');
      this.step.set('code');
    }, 'The password is not correct.');
  }

  protected async confirm(): Promise<void> {
    if (this.code.invalid) {
      this.code.markAsTouched();
      return;
    }
    await this.run(async () => {
      this.recoveryCodes.set(await this.mfa.confirmEnrolment(this.code.getRawValue().trim()));
      this.code.reset('');
      this.setupKey.set('');
      this.setupLink.set(null);
      this.step.set('recovery');
    }, 'That code is not valid.');
  }

  protected async copyCodes(): Promise<void> {
    try {
      await copyToClipboard(this.recoveryCodes().join('\n'));
      this.copied.set(true);
    } catch {
      this.errorMessage.set('Copy failed. Select the codes and copy them by hand.');
    }
  }

  protected async finish(): Promise<void> {
    if (!this.saved()) return;
    this.recoveryCodes.set([]);
    // The session was replaced by one that has the second factor; reload the profile with it.
    await this.currentUser.load();
    await this.router.navigateByUrl(this.safeReturnUrl());
  }

  private async run(action: () => Promise<void>, invalidMessage: string): Promise<void> {
    this.pending.set(true);
    this.errorMessage.set(null);
    try {
      await action();
    } catch (error) {
      const apiError = isApiError(error) ? error : mapHttpError(error);
      this.errorMessage.set(
        apiError.status === 422
          ? invalidMessage
          : apiError.status === 409
            ? 'Two-step verification is already set up for this account.'
            : mfaErrorMessage(apiError.status),
      );
    } finally {
      this.pending.set(false);
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

const isApiError = (error: unknown): error is ApiError =>
  typeof error === 'object' &&
  error !== null &&
  typeof (error as ApiError).code === 'string' &&
  typeof (error as ApiError).status === 'number';
