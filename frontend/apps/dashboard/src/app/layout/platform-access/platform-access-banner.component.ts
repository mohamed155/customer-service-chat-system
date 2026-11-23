import {
  ChangeDetectionStrategy,
  Component,
  effect,
  inject,
  signal,
  untracked,
} from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { TranslocoPipe } from '@jsverse/transloco';
import { ApiError } from '../../core/api/api.models';
import { toApiError } from '../../core/errors/api-error';
import { errorMessageKey } from '../../core/errors/error-message';
import { APP_PATHS } from '../../core/router/app-paths';
import { TenantContextService } from '../../core/tenant/tenant-context.service';
import { ButtonComponent } from '../../shared/components/button/button.component';
import { InlineAlertComponent } from '../../shared/components/inline-alert/inline-alert.component';

const MINUTE_MS = 60_000;

@Component({
  selector: 'app-platform-access-banner',
  imports: [ButtonComponent, InlineAlertComponent, RouterLink, TranslocoPipe],
  templateUrl: './platform-access-banner.component.html',
  styles: [
    `
      .banner {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-2);
        margin: var(--app-space-3) var(--app-page-padding-x) 0;
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-inline-start: 3px solid var(--app-accent);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
      }
      .banner.expired {
        border-inline-start-color: var(--app-red);
      }
      a {
        color: var(--app-accent-strong);
        font-weight: 600;
      }
      app-inline-alert {
        flex-basis: 100%;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PlatformAccessBannerComponent {
  private readonly tenantContext = inject(TenantContextService);
  private readonly router = inject(Router);

  protected readonly accessSession = this.tenantContext.accessSession;
  protected readonly minutesLeft = signal(0);
  protected readonly ending = signal(false);
  protected readonly error = signal<ApiError | null>(null);
  protected readonly errorKey = errorMessageKey;
  protected readonly tenantsPath = [`/${APP_PATHS.platform.base}`, APP_PATHS.platform.tenants];

  constructor() {
    effect((onCleanup) => {
      const session = this.accessSession();
      if (!session) return;
      const expiresAt = Date.parse(session.expiresAt);
      let timeout: ReturnType<typeof setTimeout> | undefined;
      const tick = () => {
        const remaining = Math.max(0, expiresAt - Date.now());
        this.minutesLeft.set(Math.ceil(remaining / MINUTE_MS));
        if (remaining > 0) {
          timeout = setTimeout(tick, remaining % MINUTE_MS || MINUTE_MS);
          return;
        }
        this.tenantContext.expireAccess(session.id);
        if (this.router.url.startsWith(`/${APP_PATHS.tenant.base}/`)) {
          void this.router.navigate(this.tenantsPath);
        }
      };
      untracked(tick);
      onCleanup(() => clearTimeout(timeout));
    });
  }

  protected async end(): Promise<void> {
    if (this.ending()) return;
    this.ending.set(true);
    this.error.set(null);
    try {
      await this.tenantContext.endAccess();
      await this.router.navigate(this.tenantsPath);
    } catch (error: unknown) {
      this.error.set(toApiError(error));
    } finally {
      this.ending.set(false);
    }
  }
}
