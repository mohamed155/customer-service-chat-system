import { AnalyticsSummary } from '../../core/api/tenant-api.models';
import { formatNumber } from '../../core/i18n/format';
import { channelLabelKey } from '../../core/i18n/labels';
import { LocaleService } from '../../core/i18n/locale.service';
import { marker } from '../../core/i18n/marker';
import { MetricCardData } from '../components/metric-card/metric-card.component';

/** The name of a channel in the active language; the widget has a name of its own here. */
export function analyticsChannelName(locale: LocaleService, channel: string): string {
  if (channel === 'widget') return locale.translate(marker('insights.analytics.websiteWidget'));
  const key = channelLabelKey(channel);
  return key ? locale.translate(key) : channel;
}

/**
 * The metric cards of an analytics summary, in the active language. The analytics page shows all
 * of them; the overview picks the ones it has room for by `id`.
 */
export function analyticsMetricCards(
  summary: AnalyticsSummary,
  locale: LocaleService,
): MetricCardData[] {
  const text = (key: string, params?: Record<string, unknown>): string =>
    locale.translate(key, params);
  const number = (value: number, options?: Intl.NumberFormatOptions): string =>
    formatNumber(value, locale.formatLocale(), options);
  const none = (): string => text(marker('insights.analytics.none'));
  const percent = (value: number | null): string =>
    value == null
      ? none()
      : number(value, { style: 'percent', minimumFractionDigits: 1, maximumFractionDigits: 1 });
  const seconds = (total: number | null): string => {
    if (total == null) return none();
    if (total < 60) {
      return text(marker('insights.analytics.seconds'), { seconds: Math.round(total) });
    }
    return text(marker('insights.analytics.minutesSeconds'), {
      minutes: Math.floor(total / 60),
      seconds: Math.round(total % 60),
    });
  };
  const satisfaction =
    summary.satisfactionAvg == null
      ? none()
      : text(marker('insights.analytics.satisfactionValue'), {
          avg: number(summary.satisfactionAvg, {
            minimumFractionDigits: 1,
            maximumFractionDigits: 1,
          }),
          count: summary.satisfactionCount,
        });

  return [
    {
      id: 'conversations',
      label: text(marker('insights.analytics.conversations')),
      value: number(summary.conversationVolume),
      icon: '@tui.message-square',
    },
    {
      id: 'ai-resolution-rate',
      label: text(marker('insights.analytics.aiResolutionRate')),
      value: percent(summary.aiResolutionRate),
      icon: '@tui.bot',
    },
    {
      id: 'handoffs',
      label: text(marker('insights.analytics.handoffs')),
      value: percent(summary.handoffRate),
      icon: '@tui.user-round',
    },
    {
      id: 'avg-first-response',
      label: text(marker('insights.analytics.avgFirstResponse')),
      value: seconds(summary.avgFirstResponseSeconds),
      icon: '@tui.timer',
    },
    {
      id: 'avg-response',
      label: text(marker('insights.analytics.avgResponse')),
      value: seconds(summary.avgResponseSeconds),
      icon: '@tui.clock',
    },
    {
      id: 'satisfaction',
      label: text(marker('insights.analytics.satisfaction')),
      value: satisfaction,
      icon: '@tui.star',
    },
    {
      id: 'tokens',
      label: text(marker('insights.analytics.tokens')),
      value: number(summary.totalTokens),
      icon: '@tui.zap',
    },
  ];
}
