import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { BadgeTone } from '../../../shared/components/status-badge/status-badge.component';
import { BillingStore } from './billing.store';

function formatCents(cents: number): string {
  return `$${(cents / 100).toFixed(2)}`;
}

function formatDate(d: Date): string {
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
}

function metricLabel(allowance: number | null): string {
  return allowance === null ? 'Unlimited' : allowance.toLocaleString();
}

function aiPausedMessage(reason: string | null): string {
  switch (reason) {
    case 'token_limit':
      return 'AI replies are paused — the monthly token allowance is used up.';
    case 'conversation_limit':
      return 'AI replies are paused — the monthly conversation allowance is used up.';
    case 'subscription_status':
      return 'AI replies are paused — the subscription is past due or canceled.';
    default:
      return '';
  }
}

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
    StatusBadgeComponent,
  ],
  providers: [BillingStore],
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
  protected readonly formatCents = formatCents;
  protected readonly formatDate = formatDate;
  protected readonly metricLabel = metricLabel;
  protected readonly subscriptionTone = subscriptionTone;
  protected readonly aiPausedMessage = aiPausedMessage;
}
