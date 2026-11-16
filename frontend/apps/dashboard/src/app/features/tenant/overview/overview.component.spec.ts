import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { AnalyticsApiService } from '../../../core/api/analytics-api.service';
import {
  AnalyticsSummary,
  AnalyticsTimeseries,
  Conversation,
} from '../../../core/api/tenant-api.models';
import { Permission } from '../../../core/authz/permissions';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { OverviewApiService } from './overview-api.service';
import { OverviewComponent } from './overview.component';
import { OverviewStore } from './overview.store';

const SUMMARY: AnalyticsSummary = {
  range: { from: '2026-10-18', to: '2026-11-16' },
  channel: null,
  conversationVolume: 150,
  concludedCount: 120,
  aiResolutionRate: 0.667,
  handoffRate: 0.333,
  avgFirstResponseSeconds: 45,
  avgResponseSeconds: 120,
  satisfactionAvg: 4.2,
  satisfactionCount: 42,
  totalTokens: 50000,
  unattributedTokens: 0,
  channels: [
    { channel: 'widget', conversationCount: 100, share: 0.667 },
    { channel: 'email', conversationCount: 50, share: 0.333 },
  ],
};

const TIMESERIES: AnalyticsTimeseries = {
  range: { from: '2026-11-15', to: '2026-11-16' },
  channel: null,
  days: [
    {
      date: '2026-11-15',
      conversationVolume: 50,
      aiResolved: 30,
      handedOff: 20,
      satisfactionAvg: null,
      satisfactionCount: 0,
      totalTokens: 100,
    },
    {
      date: '2026-11-16',
      conversationVolume: 60,
      aiResolved: 40,
      handedOff: 20,
      satisfactionAvg: null,
      satisfactionCount: 0,
      totalTokens: 100,
    },
  ],
};

const CONVERSATION: Conversation = {
  id: 'c1',
  customer: { id: 'cu1', displayName: 'Maya Chen' },
  channel: 'web_chat',
  status: 'open',
  assignee: null,
  lastMessage: { kind: 'customer', preview: 'Where is my order?' },
  lastActivityAt: '2026-11-16T09:00:00Z',
  createdAt: '2026-11-16T08:00:00Z',
};

describe('OverviewComponent', () => {
  const getSummary = vi.fn();
  const getTimeseries = vi.fn();
  const recentConversations = vi.fn();

  function mount(permissions: Permission[]) {
    TestBed.configureTestingModule({
      imports: [OverviewComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideRouter([]),
        { provide: AnalyticsApiService, useValue: { getSummary, getTimeseries } },
        { provide: OverviewApiService, useValue: { recentConversations } },
        {
          provide: PermissionsService,
          useValue: { has: (p: Permission) => permissions.includes(p) },
        },
      ],
    });
    const fixture = TestBed.createComponent(OverviewComponent);
    fixture.componentRef.injector.get(OverviewStore).load();
    fixture.detectChanges();
    return fixture;
  }

  const settled = async (fixture: ReturnType<typeof mount>) => {
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
    });
    return fixture.nativeElement as HTMLElement;
  };

  beforeEach(() => {
    getSummary.mockReset().mockReturnValue(of({ data: SUMMARY }));
    getTimeseries.mockReset().mockReturnValue(of({ data: TIMESERIES }));
    recentConversations.mockReset().mockReturnValue(of({ data: [CONVERSATION] }));
  });

  it('renders five real metric cards, the trend, the channel mix and the recent conversations', async () => {
    const element = await settled(mount(['analytics.view', 'conversations.view']));

    expect(element.querySelectorAll('app-metric-card')).toHaveLength(5);
    expect(element.textContent).toContain('150');
    expect(element.querySelector('app-trend-chart')).toBeTruthy();
    expect(element.querySelector('app-breakdown-bars')).toBeTruthy();
    expect(element.textContent).toContain('Website widget');
    expect(element.textContent).toContain('Maya Chen');
    expect(element.textContent).toContain('Where is my order?');
  });

  it('shows no fixture wording: no invented alert and no fixture subtitle', async () => {
    const element = await settled(mount(['analytics.view', 'conversations.view']));

    expect(element.textContent).not.toContain('Elevated AI provider latency');
    expect(element.textContent).not.toContain('fixture');
    expect(element.querySelector('app-escalation-banner')).toBeFalsy();
  });

  it('names the widget channel the same in the mix and on a conversation', async () => {
    recentConversations.mockReturnValue(of({ data: [{ ...CONVERSATION, channel: 'widget' }] }));
    const element = await settled(mount(['analytics.view', 'conversations.view']));

    const names = Array.from(element.querySelectorAll('app-channel-badge')).map((b) =>
      b.textContent?.trim(),
    );
    expect(names).toContain('Website widget');
    expect(element.querySelector('app-breakdown-bars')?.textContent).toContain('Website widget');
  });

  it('links each recent conversation to its detail page', async () => {
    const element = await settled(mount(['conversations.view']));

    const link = element.querySelector<HTMLAnchorElement>('.activity a');
    expect(link?.getAttribute('href')).toBe('/tenant/conversations/c1');
  });

  it('shows only the inbox section, and asks no analytics, without analytics.view', async () => {
    const element = await settled(mount(['conversations.view']));

    expect(getSummary).not.toHaveBeenCalled();
    expect(getTimeseries).not.toHaveBeenCalled();
    expect(element.querySelector('app-metric-card')).toBeFalsy();
    expect(element.querySelector('app-trend-chart')).toBeFalsy();
    expect(element.textContent).toContain('Maya Chen');
  });

  it('shows only the analytics sections, and asks no inbox, without conversations.view', async () => {
    const element = await settled(mount(['analytics.view']));

    expect(recentConversations).not.toHaveBeenCalled();
    expect(element.querySelectorAll('app-metric-card')).toHaveLength(5);
    expect(element.textContent).not.toContain('Recent activity');
  });

  it('explains an overview with nothing to show when neither permission is held', async () => {
    const element = await settled(mount([]));

    expect(getSummary).not.toHaveBeenCalled();
    expect(recentConversations).not.toHaveBeenCalled();
    expect(element.querySelector('app-empty-state')?.textContent).toContain(
      'Nothing to show here yet',
    );
  });

  it('keeps the other sections when one fails, and retries only that one', async () => {
    getTimeseries.mockReturnValueOnce(throwError(() => ({ status: 500, code: 'internal_error' })));
    const fixture = mount(['analytics.view', 'conversations.view']);
    const element = await settled(fixture);

    expect(element.querySelectorAll('app-metric-card')).toHaveLength(5);
    expect(element.textContent).toContain('Maya Chen');
    expect(element.querySelector('app-trend-chart')).toBeFalsy();
    const retry = Array.from(element.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Try again'),
    ) as HTMLButtonElement;
    expect(retry).toBeTruthy();

    retry.click();
    fixture.detectChanges();
    await settled(fixture);

    expect(getTimeseries).toHaveBeenCalledTimes(2);
    expect(getSummary).toHaveBeenCalledTimes(1);
    expect(element.querySelector('app-trend-chart')).toBeTruthy();
  });

  it('says so, honestly, when there are no conversations yet', async () => {
    recentConversations.mockReturnValue(of({ data: [] }));
    getSummary.mockReturnValue(of({ data: { ...SUMMARY, channels: [], conversationVolume: 0 } }));
    getTimeseries.mockReturnValue(
      of({
        data: {
          ...TIMESERIES,
          days: TIMESERIES.days.map((d) => ({ ...d, conversationVolume: 0 })),
        },
      }),
    );
    const element = await settled(mount(['analytics.view', 'conversations.view']));

    const text = element.textContent;
    expect(text).toContain('No conversations in this period');
    expect(text).toContain('No channel activity');
    expect(text).toContain('No conversations yet');
    expect(element.querySelector('app-trend-chart')).toBeFalsy();
  });
});
