import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { ApiError } from '../../../core/api/api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { formatNumber } from '../../../core/i18n/format';
import { channelLabelKey } from '../../../core/i18n/labels';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import {
  MetricCardComponent,
  MetricCardData,
} from '../../../shared/components/metric-card/metric-card.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { ToolbarComponent } from '../../../shared/components/toolbar/toolbar.component';
import {
  SelectFilterComponent,
  SelectFilterOption,
} from '../../../shared/components/select-filter/select-filter.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import {
  TrendChartComponent,
  TrendSeries,
} from '../../../shared/components/trend-chart/trend-chart.component';
import { BreakdownBarsComponent } from '../../../shared/components/breakdown-bars/breakdown-bars.component';
import { AnalyticsStore } from './analytics.store';

@Component({
  selector: 'app-analytics',
  imports: [
    DashboardCardComponent,
    EmptyStateComponent,
    ErrorReferenceComponent,
    InlineAlertComponent,
    LoadingStateComponent,
    MetricCardComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SectionHeaderComponent,
    ToolbarComponent,
    SelectFilterComponent,
    TrendChartComponent,
    BreakdownBarsComponent,
    TranslocoPipe,
  ],
  providers: [AnalyticsStore, provideTranslocoScope('insights')],
  templateUrl: './analytics.component.html',
  styles: [
    `
      .metrics {
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: var(--app-space-4);
      }
      @media (max-width: 1200px) {
        .metrics {
          grid-template-columns: repeat(2, minmax(0, 1fr));
        }
      }
      @media (max-width: 768px) {
        .metrics {
          grid-template-columns: 1fr;
        }
      }
      .charts {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: var(--app-space-4);
        margin-top: var(--app-space-6);
      }
      @media (max-width: 768px) {
        .charts {
          grid-template-columns: 1fr;
        }
      }
      .breakdown {
        margin-top: var(--app-space-6);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AnalyticsComponent {
  protected readonly store = inject(AnalyticsStore);

  private readonly locale = inject(LocaleService);

  /** `translate` with the catalog's own wording, for the labels code builds. */
  private text(key: string, params?: Record<string, unknown>): string {
    return this.locale.translate(key, params);
  }

  protected readonly datePresetOptions = computed<SelectFilterOption[]>(() => [
    { value: '7', label: this.text(marker('insights.analytics.last7')) },
    { value: '30', label: this.text(marker('insights.analytics.last30')) },
    { value: '90', label: this.text(marker('insights.analytics.last90')) },
    { value: 'custom', label: this.text(marker('insights.analytics.custom')) },
  ]);

  /** The name of a channel: the widget has a name of its own here, the rest are shared. */
  private channelName(channel: string): string {
    if (channel === 'widget') return this.text(marker('insights.analytics.websiteWidget'));
    const key = channelLabelKey(channel);
    return key ? this.text(key) : channel;
  }

  protected readonly channelOptions = computed<SelectFilterOption[]>(() => [
    { value: 'all', label: this.text(marker('insights.analytics.allChannels')) },
    ...['widget', 'email', 'phone', 'web_chat', 'whatsapp', 'telegram'].map((channel) => ({
      value: channel,
      label: this.channelName(channel),
    })),
  ]);

  protected readonly channelDisplayValue = computed(() => this.store.channel() ?? 'all');

  protected readonly breakdownItems = computed(() => {
    const channels = this.store.summary()?.channels;
    if (!channels) return [];
    return channels.map((c) => ({
      label: this.channelName(c.channel),
      count: c.conversationCount,
      share: c.share,
    }));
  });

  protected onPresetChange(value: string): void {
    this.store.setPreset(value as '7' | '30' | '90' | 'custom');
  }

  protected onFromChange(event: Event): void {
    const from = (event.target as HTMLInputElement).value;
    this.store.setCustomRange(from, this.store.to());
  }

  protected onToChange(event: Event): void {
    const to = (event.target as HTMLInputElement).value;
    this.store.setCustomRange(this.store.from(), to);
  }

  private number(value: number, options?: Intl.NumberFormatOptions): string {
    return formatNumber(value, this.locale.formatLocale(), options);
  }

  private percent(value: number | null): string {
    if (value == null) return this.text(marker('insights.analytics.none'));
    return this.number(value, {
      style: 'percent',
      minimumFractionDigits: 1,
      maximumFractionDigits: 1,
    });
  }

  private seconds(totalSeconds: number | null): string {
    if (totalSeconds == null) return this.text(marker('insights.analytics.none'));
    if (totalSeconds < 60) {
      return this.text(marker('insights.analytics.seconds'), {
        seconds: Math.round(totalSeconds),
      });
    }
    return this.text(marker('insights.analytics.minutesSeconds'), {
      minutes: Math.floor(totalSeconds / 60),
      seconds: Math.round(totalSeconds % 60),
    });
  }

  private satisfaction(avg: number | null, count: number): string {
    if (avg == null) return this.text(marker('insights.analytics.none'));
    return this.text(marker('insights.analytics.satisfactionValue'), {
      avg: this.number(avg, { minimumFractionDigits: 1, maximumFractionDigits: 1 }),
      count,
    });
  }

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  protected readonly metricCards = computed<MetricCardData[]>(() => {
    const s = this.store.summary();
    if (!s) return [];
    return [
      {
        id: 'conversations',
        label: this.text(marker('insights.analytics.conversations')),
        value: this.number(s.conversationVolume),
        icon: '@tui.message-square',
      },
      {
        id: 'ai-resolution-rate',
        label: this.text(marker('insights.analytics.aiResolutionRate')),
        value: this.percent(s.aiResolutionRate),
        icon: '@tui.bot',
      },
      {
        id: 'handoffs',
        label: this.text(marker('insights.analytics.handoffs')),
        value: this.percent(s.handoffRate),
        icon: '@tui.user-round',
      },
      {
        id: 'avg-first-response',
        label: this.text(marker('insights.analytics.avgFirstResponse')),
        value: this.seconds(s.avgFirstResponseSeconds),
        icon: '@tui.timer',
      },
      {
        id: 'avg-response',
        label: this.text(marker('insights.analytics.avgResponse')),
        value: this.seconds(s.avgResponseSeconds),
        icon: '@tui.clock',
      },
      {
        id: 'satisfaction',
        label: this.text(marker('insights.analytics.satisfaction')),
        value: this.satisfaction(s.satisfactionAvg, s.satisfactionCount),
        icon: '@tui.star',
      },
      {
        id: 'tokens',
        label: this.text(marker('insights.analytics.tokens')),
        value: this.number(s.totalTokens),
        icon: '@tui.zap',
      },
    ];
  });

  protected readonly chartLabels = computed(() => {
    const ts = this.store.timeseries();
    if (!ts) return [];
    return ts.days.map((d) => d.date);
  });

  protected readonly conversationVolumeSeries = computed<TrendSeries>(() => {
    const ts = this.store.timeseries();
    return {
      id: 'conversation-volume',
      label: this.text(marker('insights.analytics.conversations')),
      color: 'chart-1',
      points: ts ? ts.days.map((d) => d.conversationVolume) : [],
    };
  });

  protected readonly aiVsHandoffSeries = computed<readonly TrendSeries[]>(() => {
    const ts = this.store.timeseries();
    if (!ts) return [];
    return [
      {
        id: 'ai',
        label: this.text(marker('insights.analytics.aiResolved')),
        color: 'chart-1',
        points: ts.days.map((d) => d.aiResolved),
      },
      {
        id: 'handoff',
        label: this.text(marker('insights.analytics.humanHandoff')),
        color: 'chart-2',
        points: ts.days.map((d) => d.handedOff),
      },
    ];
  });

  protected readonly satisfactionSeries = computed<TrendSeries>(() => {
    const ts = this.store.timeseries();
    return {
      id: 'satisfaction',
      label: this.text(marker('insights.analytics.avgSatisfaction')),
      color: 'chart-1',
      points: ts ? ts.days.map((d) => d.satisfactionAvg) : [],
    };
  });

  protected readonly tokenSeries = computed<TrendSeries>(() => {
    const ts = this.store.timeseries();
    return {
      id: 'tokens',
      label: this.text(marker('insights.analytics.tokensSeries')),
      color: 'chart-1',
      points: ts ? ts.days.map((d) => d.totalTokens) : [],
    };
  });
}
