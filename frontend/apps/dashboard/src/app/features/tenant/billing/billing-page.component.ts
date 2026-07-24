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
  template: `
    <div class="page">
      <h1 class="page-title">Billing</h1>

      @if (store.loading()) {
        <app-loading-state label="Loading billing overview…" />
      } @else if (store.error(); as error) {
        <app-inline-alert tone="error">{{ error }}</app-inline-alert>
      } @else if (store.overview(); as ov) {
        <div class="grid">
          <app-dashboard-card>
            <div card-header>
              <span class="plan-label">Plan</span>
            </div>
            <div class="plan-info">
              <span class="plan-name">{{ ov.plan.name }}</span>
              <span class="plan-price">{{ formatCents(ov.plan.monthlyPriceCents) }}/mo</span>
              <app-status-badge
                [status]="ov.subscriptionStatus"
                [tone]="subscriptionTone(ov.subscriptionStatus)"
              />
            </div>
            <div card-footer>
              <span class="period">{{ formatDate(ov.period.start) }} – {{ formatDate(ov.period.end) }}</span>
            </div>
          </app-dashboard-card>

          <app-dashboard-card>
            <div card-header>
              <span class="metric-title">Tokens</span>
            </div>
            <div class="metric-row">
              <span class="metric-value">{{ ov.usage.tokens.used.toLocaleString() }}</span>
              <span class="metric-divider">/</span>
              <span class="metric-allowance">{{ metricLabel(ov.usage.tokens.allowance) }}</span>
            </div>
            <div class="bar-track">
              <div class="bar-fill" [style.width.%]="ov.usage.tokens.percent ?? 0"></div>
            </div>
            <div class="metric-footnote">{{ (ov.usage.tokens.percent ?? 0).toFixed(1) }}% used</div>
          </app-dashboard-card>

          <app-dashboard-card>
            <div card-header>
              <span class="metric-title">Conversations</span>
            </div>
            <div class="metric-row">
              <span class="metric-value">{{ ov.usage.conversations.used.toLocaleString() }}</span>
              <span class="metric-divider">/</span>
              <span class="metric-allowance">{{ metricLabel(ov.usage.conversations.allowance) }}</span>
            </div>
            <div class="bar-track">
              <div class="bar-fill" [style.width.%]="ov.usage.conversations.percent ?? 0"></div>
            </div>
            <div class="metric-footnote">{{ (ov.usage.conversations.percent ?? 0).toFixed(1) }}% used</div>
          </app-dashboard-card>
        </div>
      }
    </div>
  `,
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
}
