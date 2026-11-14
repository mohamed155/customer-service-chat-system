import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { RelativeTimePipe } from '../../../core/i18n/relative-time.pipe';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { ChannelBadgeComponent } from '../../../shared/components/channel-badge/channel-badge.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { EscalationBannerComponent } from '../../../shared/components/ai/escalation-banner/escalation-banner.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PAGE_ROUTE, RoutedPageStore } from '../../../core/routed-page/routed-page.store';
import { MetricCardComponent } from '../../../shared/components/metric-card/metric-card.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { ConversationFixture } from '../../../shared/fixtures/fixture.models';
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
    EmptyStateComponent,
    EscalationBannerComponent,
    LoadingStateComponent,
    MetricCardComponent,
    OverviewChannelBreakdownComponent,
    OverviewTrendChartComponent,
    PageContainerComponent,
    PageHeaderComponent,
    RelativeTimePipe,
    SectionHeaderComponent,
    StatusBadgeComponent,
    TranslocoPipe,
  ],
  providers: [
    RoutedPageStore,
    { provide: PAGE_ROUTE, useValue: 'overview' },
    provideTranslocoScope('insights'),
  ],
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
  protected readonly page = inject(RoutedPageStore);
  private readonly locale = inject(LocaleService);
  protected readonly hasData = computed(() => this.page.data() !== undefined);
  protected readonly hasError = computed(() => this.page.error() !== null);
  protected readonly alertDismissed = signal(false);

  protected readonly pageData = computed(() => {
    const data = this.page.data();
    if (data?.page === 'overview') return data.data;
    return undefined;
  });

  protected customerName(conversation: ConversationFixture): string {
    const customers = this.pageData()?.customers;
    const fallback = this.locale.translate(marker('insights.overview.customer'));
    if (!customers) return fallback;
    return customers.find((c) => c.id === conversation.customerId)?.name ?? fallback;
  }

  protected customerInitials(conversation: ConversationFixture): string {
    const customers = this.pageData()?.customers;
    if (!customers) return 'HC';
    return customers.find((c) => c.id === conversation.customerId)?.avatarInitials ?? 'HC';
  }

  protected statusTone(status: ConversationFixture['status']): 'green' | 'amber' | 'red' {
    return status === 'closed' ? 'green' : status === 'escalated' ? 'red' : 'amber';
  }

  /** A conversation status's name in the active language; an unknown one shows as sent. */
  protected statusName(status: ConversationFixture['status']): string {
    const keys: Record<string, string> = {
      open: marker('status.open'),
      pending: marker('status.pending'),
      resolved: marker('status.resolved'),
      closed: marker('status.closed'),
      escalated: marker('insights.overview.escalated'),
    };
    const key = Object.prototype.hasOwnProperty.call(keys, status) ? keys[status] : null;
    return key ? this.locale.translate(key) : status;
  }

  protected retry(): void {
    this.page.retry();
  }
}
