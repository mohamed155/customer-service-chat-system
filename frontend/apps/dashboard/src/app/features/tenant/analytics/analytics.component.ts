import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
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

const CHANNEL_LABELS: Record<string, string> = {
  widget: 'Website widget',
  email: 'Email',
  phone: 'Phone',
  web_chat: 'Web chat',
  whatsapp: 'WhatsApp',
  telegram: 'Telegram',
};

function formatPercent(value: number | null): string {
  if (value == null) return '\u2014';
  return `${(value * 100).toFixed(1)}%`;
}

function formatSeconds(totalSeconds: number | null): string {
  if (totalSeconds == null) return '\u2014';
  if (totalSeconds < 60) return `${Math.round(totalSeconds)}s`;
  const m = Math.floor(totalSeconds / 60);
  const s = Math.round(totalSeconds % 60);
  return `${m}m ${s}s`;
}

function formatSatisfaction(avg: number | null, count: number): string {
  if (avg == null) return '\u2014';
  return `${avg.toFixed(1)} / 5 (${count} ratings)`;
}

@Component({
  selector: 'app-analytics',
  imports: [
    DashboardCardComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    MetricCardComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SectionHeaderComponent,
    ToolbarComponent,
    SelectFilterComponent,
    TrendChartComponent,
    BreakdownBarsComponent,
  ],
  providers: [AnalyticsStore],
  template: `
    <app-page-container>
      <app-page-header title="Analytics" description="Trends across every channel" />
      <app-toolbar>
        <div toolbar-start>
          <app-select-filter
            label="Date range"
            [value]="store.preset()"
            [options]="datePresetOptions"
            (valueChange)="onPresetChange($event)"
          />
          @if (store.preset() === 'custom') {
            <input type="date" [value]="store.from()" (change)="onFromChange($event)" />
            <span>to</span>
            <input type="date" [value]="store.to()" (change)="onToChange($event)" />
          }
          <app-select-filter
            label="Channel"
            [value]="channelDisplayValue()"
            [options]="channelOptions"
            (valueChange)="store.setChannel($event)"
          />
        </div>
      </app-toolbar>
      @if (store.loading()) {
        <app-loading-state />
      } @else if (store.error(); as errMsg) {
        <app-empty-state
          icon="@tui.alert-circle"
          title="Something went wrong"
          [description]="errMsg"
        >
          <button type="button" (click)="store.load()">Try again</button>
        </app-empty-state>
      } @else if (store.summary(); as summary) {
        <section class="metrics">
          @for (card of metricCards(); track card.id) {
            <app-metric-card [metric]="card" />
          }
        </section>
        @if (store.timeseries(); as timeseries) {
          <section class="charts">
            <app-dashboard-card>
              <app-section-header card-header title="Conversation volume" />
              <app-trend-chart [series]="[conversationVolumeSeries()]" [labels]="chartLabels()" />
            </app-dashboard-card>
            <app-dashboard-card>
              <app-section-header card-header title="AI resolved vs human handoff" />
              <app-trend-chart [series]="aiVsHandoffSeries()" [labels]="chartLabels()" />
            </app-dashboard-card>
            <app-dashboard-card>
              <app-section-header card-header title="Satisfaction trend" />
              <app-trend-chart [series]="[satisfactionSeries()]" [labels]="chartLabels()" />
            </app-dashboard-card>
            <app-dashboard-card>
              <app-section-header card-header title="Token usage" />
              <app-trend-chart [series]="[tokenSeries()]" [labels]="chartLabels()" />
            </app-dashboard-card>
          </section>
          <section class="breakdown">
            <app-dashboard-card>
              <app-section-header card-header title="Channel breakdown" />
              <app-breakdown-bars [items]="breakdownItems()" />
            </app-dashboard-card>
          </section>
        }
      } @else {
        <app-empty-state
          icon="@tui.chart-line"
          title="No analytics data"
          description="Reports and charts will populate as customer interactions are recorded."
        />
      }
    </app-page-container>
  `,
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

  protected readonly datePresetOptions: SelectFilterOption[] = [
    { value: '7', label: 'Last 7 days' },
    { value: '30', label: 'Last 30 days' },
    { value: '90', label: 'Last 90 days' },
    { value: 'custom', label: 'Custom range' },
  ];

  protected readonly channelOptions: SelectFilterOption[] = [
    { value: 'all', label: 'All channels' },
    { value: 'widget', label: 'Website widget' },
    { value: 'email', label: 'Email' },
    { value: 'phone', label: 'Phone' },
    { value: 'web_chat', label: 'Web chat' },
    { value: 'whatsapp', label: 'WhatsApp' },
    { value: 'telegram', label: 'Telegram' },
  ];

  protected readonly channelDisplayValue = computed(() => this.store.channel() ?? 'all');

  protected readonly breakdownItems = computed(() => {
    const channels = this.store.summary()?.channels;
    if (!channels) return [];
    return channels.map((c) => ({
      label: CHANNEL_LABELS[c.channel] ?? c.channel,
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

  protected readonly metricCards = computed<MetricCardData[]>(() => {
    const s = this.store.summary();
    if (!s) return [];
    return [
      {
        id: 'conversations',
        label: 'Conversations',
        value: String(s.conversationVolume),
        icon: '@tui.message-square',
      },
      {
        id: 'ai-resolution-rate',
        label: 'AI resolution rate',
        value: formatPercent(s.aiResolutionRate),
        icon: '@tui.bot',
      },
      {
        id: 'handoffs',
        label: 'Human handoffs',
        value: formatPercent(s.handoffRate),
        icon: '@tui.user-round',
      },
      {
        id: 'avg-first-response',
        label: 'Avg first response',
        value: formatSeconds(s.avgFirstResponseSeconds),
        icon: '@tui.timer',
      },
      {
        id: 'avg-response',
        label: 'Avg response',
        value: formatSeconds(s.avgResponseSeconds),
        icon: '@tui.clock',
      },
      {
        id: 'satisfaction',
        label: 'Satisfaction',
        value: formatSatisfaction(s.satisfactionAvg, s.satisfactionCount),
        icon: '@tui.star',
      },
      {
        id: 'tokens',
        label: 'Tokens used',
        value: s.totalTokens.toLocaleString(),
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
      label: 'Conversations',
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
        label: 'AI resolved',
        color: 'chart-1',
        points: ts.days.map((d) => d.aiResolved),
      },
      {
        id: 'handoff',
        label: 'Human handoff',
        color: 'chart-2',
        points: ts.days.map((d) => d.handedOff),
      },
    ];
  });

  protected readonly satisfactionSeries = computed<TrendSeries>(() => {
    const ts = this.store.timeseries();
    return {
      id: 'satisfaction',
      label: 'Avg satisfaction',
      color: 'chart-1',
      points: ts ? ts.days.map((d) => d.satisfactionAvg) : [],
    };
  });

  protected readonly tokenSeries = computed<TrendSeries>(() => {
    const ts = this.store.timeseries();
    return {
      id: 'tokens',
      label: 'Tokens',
      color: 'chart-1',
      points: ts ? ts.days.map((d) => d.totalTokens) : [],
    };
  });
}
