import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { ApiError } from '../../../core/api/api.models';
import { BILLING_PRICE_FORMAT } from '../../../core/billing/billing-currency';
import { errorMessageKey } from '../../../core/errors/error-message';
import { LocaleDatePipe, LocaleNumberPipe } from '../../../core/i18n/format';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { ownValue } from '../../../core/i18n/own-value';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { BadgeTone } from '../../../shared/components/status-badge/status-badge.component';
import { BillingStore } from './billing.store';

/** Catalog keys of why AI replies are paused (`ai_paused_reason`). */
const PAUSED_KEYS: Readonly<Record<string, string>> = {
  token_limit: marker('settings.billingPage.pausedTokenLimit'),
  conversation_limit: marker('settings.billingPage.pausedConversationLimit'),
  subscription_status: marker('settings.billingPage.pausedSubscription'),
};

/** Catalog keys of the subscription statuses. */
const SUBSCRIPTION_KEYS: Readonly<Record<string, string>> = {
  active: marker('settings.billingPage.subscription.active'),
  trialing: marker('settings.billingPage.subscription.trialing'),
  past_due: marker('settings.billingPage.subscription.pastDue'),
  canceled: marker('settings.billingPage.subscription.canceled'),
  incomplete: marker('settings.billingPage.subscription.incomplete'),
  incomplete_expired: marker('settings.billingPage.subscription.incompleteExpired'),
};

function subscriptionTone(status: string): BadgeTone {
  switch (status) {
    case 'active':
      return 'green';
    case 'trialing':
      return 'accent';
    case 'past_due':
      return 'amber';
    case 'canceled':
    case 'incomplete':
    case 'incomplete_expired':
      return 'red';
    default:
      return 'neutral';
  }
}

@Component({
  selector: 'app-billing-page',
  standalone: true,
  imports: [
    DashboardCardComponent,
    InlineAlertComponent,
    LoadingStateComponent,
    LocaleDatePipe,
    LocaleNumberPipe,
    StatusBadgeComponent,
    TranslocoPipe,
  ],
  providers: [BillingStore, provideTranslocoScope('settings')],
  templateUrl: './billing-page.component.html',
  styles: [
    `
      .page {
        padding: var(--app-space-4);
        max-width: 720px;
      }
      .page-title {
        margin: 0 0 var(--app-space-5);
        font-size: var(--app-font-2xl);
        font-weight: 700;
        color: var(--app-text);
      }
      .grid {
        display: flex;
        flex-direction: column;
        gap: var(--app-space-4);
      }
      .plan-label,
      .metric-title {
        font-size: var(--app-font-sm);
        font-weight: 600;
        color: var(--app-text-2);
        text-transform: uppercase;
        letter-spacing: 0.04em;
      }
      .plan-info {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        flex-wrap: wrap;
      }
      .plan-name {
        font-size: var(--app-font-xl);
        font-weight: 700;
        color: var(--app-text);
      }
      .plan-price {
        font-size: var(--app-font-md);
        color: var(--app-text-2);
      }
      .period {
        font-size: var(--app-font-sm);
        color: var(--app-text-2);
      }
      .metric-row {
        display: flex;
        align-items: baseline;
        gap: var(--app-space-1);
        margin-bottom: var(--app-space-3);
      }
      .metric-value {
        font-size: var(--app-font-2xl);
        font-weight: 700;
        color: var(--app-text);
      }
      .metric-divider {
        font-size: var(--app-font-md);
        color: var(--app-text-3);
      }
      .metric-allowance {
        font-size: var(--app-font-md);
        font-weight: 600;
        color: var(--app-text-2);
      }
      .bar-track {
        height: 8px;
        background: var(--app-border);
        border-radius: var(--app-radius-sm);
        overflow: hidden;
        margin-bottom: var(--app-space-2);
      }
      .bar-fill {
        height: 100%;
        background: var(--app-accent);
        border-radius: var(--app-radius-sm);
        transition: width 0.3s ease;
      }
      .metric-footnote {
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
      }
      .warning .bar-fill {
        background: var(--app-amber);
      }
      .exceeded .bar-fill {
        background: var(--app-red);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BillingPageComponent {
  protected readonly store = inject(BillingStore);
  private readonly locale = inject(LocaleService);
  protected readonly subscriptionTone = subscriptionTone;
  protected readonly currencyFormat = BILLING_PRICE_FORMAT;
  protected readonly percentFormat = {
    style: 'percent',
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  } as const;

  protected pausedKey(reason: string | null): string | null {
    return reason ? (ownValue(PAUSED_KEYS, reason) ?? null) : null;
  }

  protected subscriptionKey(status: string): string | null {
    return ownValue(SUBSCRIPTION_KEYS, status) ?? null;
  }

  /** The name from the catalog, or the API's own word for a status this app has no name for. */
  protected nameOf(key: string | null, fallback: string): string {
    return key ? this.locale.translate(key) : fallback;
  }

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }
}
