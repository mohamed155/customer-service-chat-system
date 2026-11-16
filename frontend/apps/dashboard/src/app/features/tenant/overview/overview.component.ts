import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { ApiError } from '../../../core/api/api.models';
import { Conversation } from '../../../core/api/tenant-api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { ownValue } from '../../../core/i18n/own-value';
import { RelativeTimePipe } from '../../../core/i18n/relative-time.pipe';
import { APP_PATHS } from '../../../core/router/app-paths';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { BreakdownBarsComponent } from '../../../shared/components/breakdown-bars/breakdown-bars.component';
import {
  ChannelBadgeChannel,
  ChannelBadgeComponent,
} from '../../../shared/components/channel-badge/channel-badge.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import {
  MetricCardComponent,
  MetricCardData,
} from '../../../shared/components/metric-card/metric-card.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import {
  TrendChartComponent,
  TrendSeries,
} from '../../../shared/components/trend-chart/trend-chart.component';
import {
  analyticsChannelName,
  analyticsMetricCards,
} from '../../../shared/utils/analytics-presentation';
import { OverviewStore, OVERVIEW_DAYS } from './overview.store';

/** The metrics the overview has room for; the analytics page shows all of them. */
const OVERVIEW_METRIC_IDS: readonly string[] = [
  'conversations',
  'ai-resolution-rate',
  'handoffs',
  'avg-first-response',
  'satisfaction',
];

const STATUS_KEYS: Readonly<Record<string, string>> = {
  open: marker('status.open'),
  pending: marker('status.pending'),
  resolved: marker('status.resolved'),
  closed: marker('status.closed'),
};

@Component({
  selector: 'app-overview',
  imports: [
    AvatarComponent,
    BreakdownBarsComponent,
    ChannelBadgeComponent,
    DashboardCardComponent,
    EmptyStateComponent,
    ErrorReferenceComponent,
    LoadingStateComponent,
    MetricCardComponent,
    PageContainerComponent,
    PageHeaderComponent,
    RelativeTimePipe,
    RouterLink,
    SectionHeaderComponent,
    StatusBadgeComponent,
    TranslocoPipe,
    TrendChartComponent,
  ],
  providers: [OverviewStore, provideTranslocoScope('insights')],
  templateUrl: './overview.component.html',
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
      .activity a {
        display: grid;
        grid-template-columns: auto 1fr auto;
        align-items: center;
        gap: var(--app-space-3);
        padding: var(--app-space-3);
        border-radius: var(--app-radius-md);
        color: inherit;
        text-decoration: none;
      }
      .activity a:hover {
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
      .activity-copy .preview {
        overflow: hidden;
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .activity-copy .badges {
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
        .activity a {
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
  protected readonly store = inject(OverviewStore);
  private readonly locale = inject(LocaleService);
  protected readonly days = OVERVIEW_DAYS;

  protected readonly metrics = computed<MetricCardData[]>(() => {
    const summary = this.store.summary().data;
    if (!summary) return [];
    return analyticsMetricCards(summary, this.locale).filter((card) =>
      OVERVIEW_METRIC_IDS.includes(card.id),
    );
  });

  protected readonly chartLabels = computed(
    () => this.store.timeseries().data?.days.map((day) => day.date) ?? [],
  );

  protected readonly volumeSeries = computed<readonly TrendSeries[]>(() => {
    const days = this.store.timeseries().data?.days ?? [];
    return [
      {
        id: 'conversation-volume',
        label: this.locale.translate(marker('insights.analytics.conversations')),
        color: 'chart-1',
        points: days.map((day) => day.conversationVolume),
      },
    ];
  });

  protected readonly breakdownItems = computed(() =>
    (this.store.summary().data?.channels ?? []).map((channel) => ({
      label: analyticsChannelName(this.locale, channel.channel),
      count: channel.conversationCount,
      share: channel.share,
    })),
  );

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  protected conversationLink(conversation: Conversation): string {
    return `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.conversationDetail(conversation.id)}`;
  }

  protected channelBadge(channel: string): ChannelBadgeChannel {
    return channel as ChannelBadgeChannel;
  }

  protected customerInitials(conversation: Conversation): string {
    return conversation.customer.displayName
      .split(' ')
      .map((part) => part[0])
      .join('')
      .toUpperCase()
      .slice(0, 2);
  }

  protected statusTone(status: Conversation['status']): 'green' | 'amber' | 'red' | 'neutral' {
    if (status === 'closed' || status === 'resolved') return 'green';
    if (status === 'pending') return 'red';
    return 'amber';
  }

  /** A conversation status's name in the active language; an unknown one shows as sent. */
  protected statusName(status: string): string {
    const key = ownValue(STATUS_KEYS, status);
    return key ? this.locale.translate(key) : status;
  }
}
