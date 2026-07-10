import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { ChannelBadgeComponent } from '../../../shared/components/channel-badge/channel-badge.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EscalationBannerComponent } from '../../../shared/components/ai/escalation-banner/escalation-banner.component';
import { MetricCardComponent } from '../../../shared/components/metric-card/metric-card.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import {
  CHANNEL_BREAKDOWN,
  OVERVIEW_METRICS,
  OVERVIEW_TREND_SERIES,
} from '../../../shared/fixtures/analytics.fixtures';
import { CONVERSATION_FIXTURES } from '../../../shared/fixtures/conversation.fixtures';
import { CUSTOMER_FIXTURES } from '../../../shared/fixtures/customer.fixtures';
import { ConversationFixture } from '../../../shared/fixtures/fixture.models';
import { OVERVIEW_ALERT } from '../../../shared/fixtures/settings.fixtures';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { OverviewChannelBreakdownComponent } from './overview-channel-breakdown.component';
import { OverviewTrendChartComponent } from './overview-trend-chart.component';

@Component({
  selector: 'app-overview',
  imports: [
    AvatarComponent,
    ChannelBadgeComponent,
    DashboardCardComponent,
    EscalationBannerComponent,
    MetricCardComponent,
    OverviewChannelBreakdownComponent,
    OverviewTrendChartComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
  ],
  template: `
    <app-page-container>
      <app-page-header title="Overview" />
      <div class="overview">
        @if (!alertDismissed()) {
          <app-escalation-banner
            [title]="alert.title"
            [description]="alert.description"
            (dismissed)="alertDismissed.set(true)"
          />
        }

        <section class="metrics" aria-label="Overview metrics">
          @for (metric of metrics; track metric.id) {
            <app-metric-card [metric]="metric" />
          }
        </section>

        <section class="dashboard-grid">
          <app-dashboard-card>
            <app-section-header
              card-header
              title="Conversation trends"
              subtitle="Volume, AI resolution, and escalations over the last 12 periods"
            />
            <app-overview-trend-chart [series]="trendSeries" />
          </app-dashboard-card>

          <app-dashboard-card>
            <app-section-header
              card-header
              title="Channel mix"
              subtitle="Where customers are asking for help"
            />
            <app-overview-channel-breakdown [breakdown]="breakdown" />
          </app-dashboard-card>
        </section>

        <app-dashboard-card>
          <app-section-header
            card-header
            title="Recent activity"
            subtitle="Live inbox preview from fixture conversations"
          />
          <div class="activity">
            @for (conversation of recentConversations; track conversation.id) {
              <article>
                <app-avatar [initials]="customerInitials(conversation)" size="md" />
                <div class="activity-copy">
                  <strong>{{ customerName(conversation) }}</strong>
                  <span>{{ conversation.snippet }}</span>
                  <div>
                    <app-channel-badge [channel]="conversation.channel" />
                    <app-status-badge
                      [status]="conversation.status"
                      [tone]="statusTone(conversation.status)"
                    />
                  </div>
                </div>
                <time>{{ relativeTime(conversation.updatedAt) }}</time>
              </article>
            }
          </div>
        </app-dashboard-card>
      </div>
    </app-page-container>
  `,
  styles: [
    `
      .overview {
        display: grid;
        gap: var(--app-space-5);
      }
      .metrics {
        display: grid;
        grid-template-columns: repeat(5, minmax(0, 1fr));
        gap: var(--app-space-4);
      }
      .dashboard-grid {
        display: grid;
        grid-template-columns: minmax(0, 1.45fr) minmax(320px, 0.7fr);
        gap: var(--app-space-4);
      }
      .activity {
        display: grid;
        gap: var(--app-space-1);
      }
      article {
        display: grid;
        grid-template-columns: auto 1fr auto;
        align-items: center;
        gap: var(--app-space-3);
        padding: var(--app-space-3);
        border-radius: var(--app-radius-md);
      }
      article:hover {
        background: var(--app-panel-2);
      }
      .activity-copy {
        min-width: 0;
        display: grid;
        gap: 6px;
      }
      .activity-copy strong {
        color: var(--app-text);
        font-size: var(--app-font-sm);
      }
      .activity-copy span {
        overflow: hidden;
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .activity-copy div {
        display: flex;
        gap: var(--app-space-2);
        flex-wrap: wrap;
      }
      time {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      @media (max-width: 1200px) {
        .metrics {
          grid-template-columns: repeat(3, minmax(0, 1fr));
        }
        .dashboard-grid {
          grid-template-columns: 1fr;
        }
      }
      @media (max-width: 768px) {
        .metrics {
          grid-template-columns: 1fr;
        }
        article {
          grid-template-columns: auto 1fr;
        }
        time {
          grid-column: 2;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class OverviewComponent {
  protected readonly alertDismissed = signal(false);
  protected readonly alert = OVERVIEW_ALERT;
  protected readonly metrics = OVERVIEW_METRICS;
  protected readonly trendSeries = OVERVIEW_TREND_SERIES;
  protected readonly breakdown = CHANNEL_BREAKDOWN;
  protected readonly recentConversations = CONVERSATION_FIXTURES.slice(0, 5);

  protected customerName(conversation: ConversationFixture): string {
    return (
      CUSTOMER_FIXTURES.find((customer) => customer.id === conversation.customerId)?.name ??
      'Customer'
    );
  }

  protected customerInitials(conversation: ConversationFixture): string {
    return (
      CUSTOMER_FIXTURES.find((customer) => customer.id === conversation.customerId)
        ?.avatarInitials ?? 'HC'
    );
  }

  protected statusTone(status: ConversationFixture['status']): 'green' | 'amber' | 'red' {
    return status === 'closed' ? 'green' : status === 'escalated' ? 'red' : 'amber';
  }

  protected relativeTime(iso: string): string {
    const hours = Math.max(1, Math.round((Date.now() - new Date(iso).getTime()) / 3_600_000));
    return hours < 24 ? `${hours}h ago` : `${Math.round(hours / 24)}d ago`;
  }
}
