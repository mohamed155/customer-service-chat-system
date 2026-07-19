import { ChannelBreakdownFixture, MetricFixture, TrendSeriesFixture } from './fixture.models';

export const OVERVIEW_METRICS: readonly MetricFixture[] = [
  {
    id: 'total-conversations',
    label: 'Total conversations',
    value: '1,284',
    delta: '+12.4%',
    deltaDirection: 'up',
    deltaPositive: true,
    icon: '@tui.messages-square',
    trend: [34, 42, 39, 55, 61, 68, 63, 76, 84],
  },
  {
    id: 'resolved-by-ai',
    label: 'Resolved by AI',
    value: '92%',
    delta: '+6.1%',
    deltaDirection: 'up',
    deltaPositive: true,
    icon: '@tui.bot',
    trend: [58, 61, 64, 70, 68, 75, 79, 82, 92],
  },
  {
    id: 'escalation-rate',
    label: 'Escalation rate',
    value: '8.4%',
    delta: '-2.8%',
    deltaDirection: 'down',
    deltaPositive: true,
    icon: '@tui.triangle-alert',
    trend: [18, 15, 16, 14, 12, 10, 11, 9, 8],
  },
  {
    id: 'avg-response',
    label: 'Avg. response',
    value: '38s',
    delta: '-11s',
    deltaDirection: 'down',
    deltaPositive: true,
    icon: '@tui.timer',
    trend: [62, 58, 49, 51, 45, 41, 39, 40, 38],
  },
  {
    id: 'satisfaction',
    label: 'Satisfaction',
    value: '4.8/5',
    delta: '+0.3',
    deltaDirection: 'up',
    deltaPositive: true,
    icon: '@tui.heart',
    trend: [3.9, 4.1, 4.2, 4.2, 4.4, 4.5, 4.7, 4.8, 4.8],
  },
];

export const OVERVIEW_TREND_SERIES: readonly TrendSeriesFixture[] = [
  {
    id: 'conversations',
    label: 'Conversations',
    colorToken: 'accent',
    points: [120, 142, 135, 164, 188, 176, 202, 218, 236, 251, 244, 268],
  },
  {
    id: 'ai-resolved',
    label: 'AI resolved',
    colorToken: 'green',
    points: [82, 98, 101, 126, 144, 138, 161, 177, 198, 216, 213, 235],
  },
  {
    id: 'escalations',
    label: 'Escalations',
    colorToken: 'red',
    points: [22, 21, 24, 19, 18, 17, 16, 18, 15, 14, 13, 12],
  },
];

export const CHANNEL_BREAKDOWN: readonly ChannelBreakdownFixture[] = [
  { channel: 'web', label: 'Website', percentage: 44 },
  { channel: 'whatsapp', label: 'WhatsApp', percentage: 28 },
  { channel: 'telegram', label: 'Telegram', percentage: 18 },
  { channel: 'mobile-sdk', label: 'Mobile SDK', percentage: 10 },
];
