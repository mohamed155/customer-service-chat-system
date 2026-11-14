import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { ApiError } from '../../core/api/api.models';
import { MfaService, MfaStatus, mfaErrorKey } from '../../core/auth/mfa.service';
import { mapHttpError } from '../../core/errors/http-error.mapper';
import { marker } from '../../core/i18n/marker';
import { APP_PATHS } from '../../core/router/app-paths';
import { ButtonComponent } from '../../shared/components/button/button.component';
import { InlineAlertComponent } from '../../shared/components/inline-alert/inline-alert.component';
import { copyToClipboard } from '../../shared/utils/clipboard';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

/**
 * The signed-in user's own two-step verification: whether it is on, how many recovery codes are
 * left, and replacing them. Replacing needs a fresh code; the step-up prompt asks for it.
 */
@Component({
  selector: 'app-security',
  providers: [provideTranslocoScope('auth')],
  imports: [ButtonComponent, InlineAlertComponent, RouterLink, TranslocoPipe],
  templateUrl: './security.component.html',
  styles: [
    `
      .page {
        padding: var(--app-page-padding-y) var(--app-page-padding-x);
        max-width: 40rem;
      }
      h2 {
        margin: 0 0 var(--app-space-4);
        font-size: var(--app-font-xl);
        color: var(--app-text);
      }
      section {
        display: grid;
        gap: var(--app-space-3);
      }
      ul {
        list-style: none;
        padding: 0;
        columns: 2;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SecurityComponent implements OnInit {
  private readonly mfa = inject(MfaService);

  protected readonly status = signal<MfaStatus | null>(null);
  protected readonly codes = signal<string[]>([]);
  protected readonly busy = signal(false);
  protected readonly copied = signal(false);
  /** Catalog key of what went wrong, or `null`. */
  protected readonly errorKey = signal<string | null>(null);
  /** The request reference of the failed call, shown with the message (MOH-64). */
  protected readonly errorRequestId = signal<string | null>(null);
  protected readonly setupUrl = `/${APP_PATHS.mfaSetup}`;

  async ngOnInit(): Promise<void> {
    await this.refresh();
  }

  protected async regenerate(): Promise<void> {
    this.busy.set(true);
    this.errorKey.set(null);
    this.errorRequestId.set(null);
    try {
      this.codes.set(await this.mfa.regenerateRecoveryCodes());
      this.copied.set(false);
      await this.refresh();
    } catch (error) {
      const apiError = isApiError(error) ? error : mapHttpError(error);
      // A cancelled step-up prompt is not worth an error message.
      if (apiError.code !== 'mfa_step_up_required') {
        this.errorRequestId.set(apiError.requestId ?? null);
        this.errorKey.set(mfaErrorKey(apiError.status));
      }
    } finally {
      this.busy.set(false);
    }
  }

  protected async copy(): Promise<void> {
    try {
      await copyToClipboard(this.codes().join('\n'));
      this.copied.set(true);
    } catch {
      this.errorRequestId.set(null);
      this.errorKey.set(marker('auth.recoveryCodes.copyFailed'));
    }
  }

  protected dismiss(): void {
    this.codes.set([]);
  }

  private async refresh(): Promise<void> {
    try {
      this.status.set(await this.mfa.status());
    } catch (error) {
      this.errorRequestId.set((isApiError(error) ? error : mapHttpError(error)).requestId ?? null);
      this.errorKey.set(marker('auth.security.loadFailed'));
    }
  }
}

const isApiError = (error: unknown): error is ApiError =>
  typeof error === 'object' &&
  error !== null &&
  typeof (error as ApiError).code === 'string' &&
  typeof (error as ApiError).status === 'number';
