import { provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MockStore, provideMockStore } from '@ngrx/store/testing';
import { Permission } from '../../../core/authz/permissions';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { APP_CONFIG } from '../../../core/config/app-config';
import { tenantContextInterceptor } from '../../../core/http/tenant-context.interceptor';
import {
  OVERVIEW_DAYS,
  OverviewStore,
  overviewRange,
  RECENT_CONVERSATIONS,
} from './overview.store';

const SUMMARY_URL = '/api/v1/tenant/analytics/summary';
const TIMESERIES_URL = '/api/v1/tenant/analytics/timeseries';
const CONVERSATIONS_URL = '/api/v1/tenant/conversations';

const SUMMARY_WIRE = {
  range: { from: '2026-10-18', to: '2026-11-16' },
  channel: null,
  conversation_volume: 12,
  concluded_count: 9,
  ai_resolution_rate: 0.5,
  handoff_rate: 0.25,
  avg_first_response_seconds: 30,
  avg_response_seconds: 90,
  satisfaction_avg: 4.5,
  satisfaction_count: 4,
  total_tokens: 1000,
  unattributed_tokens: 0,
  channels: [{ channel: 'widget', conversation_count: 12, share: 1 }],
};

const TIMESERIES_WIRE = {
  range: { from: '2026-11-15', to: '2026-11-16' },
  channel: null,
  days: [
    {
      date: '2026-11-15',
      conversation_volume: 5,
      ai_resolved: 3,
      handed_off: 1,
      satisfaction_avg: null,
      satisfaction_count: 0,
      total_tokens: 10,
    },
  ],
};

const CONVERSATION_WIRE = {
  id: 'c1',
  customer: { id: 'cu1', display_name: 'Maya Chen' },
  channel: 'web_chat',
  status: 'open',
  assignee: null,
  last_message: { kind: 'customer', preview: 'Where is my order?' },
  last_activity_at: '2026-11-16T09:00:00Z',
  created_at: '2026-11-16T08:00:00Z',
};

function tenantState(id: string | null) {
  return {
    tenantContext: {
      activeTenant: id ? { id, name: id, slug: id, status: 'active' } : null,
      status: 'idle' as const,
    },
  };
}

describe('OverviewStore', () => {
  let http: HttpTestingController;
  let mockStore: MockStore;
  const granted = signal(new Set<Permission>());

  function setup(permissions: Permission[], tenantId: string | null = 'tenant-a') {
    granted.set(new Set(permissions));
    TestBed.configureTestingModule({
      providers: [
        OverviewStore,
        provideHttpClient(withInterceptors([tenantContextInterceptor])),
        provideHttpClientTesting(),
        {
          provide: APP_CONFIG,
          useValue: {
            apiBaseUrl: '/api/v1',
            appName: 'Dashboard',
            environmentName: 'development',
            publicDashboardUrl: 'http://localhost:4200',
          },
        },
        { provide: PermissionsService, useValue: { has: (p: Permission) => granted().has(p) } },
        provideMockStore({ initialState: tenantState(tenantId) }),
      ],
    });
    http = TestBed.inject(HttpTestingController);
    mockStore = TestBed.inject(MockStore);
    return TestBed.inject(OverviewStore);
  }

  afterEach(() => http.verify());

  const urlIs = (url: string) => (r: { url: string }) => r.url === url;

  it('asks the three sources on creation and sends the active tenant id to each', () => {
    const store = setup(['analytics.view', 'conversations.view'], 'tenant-a');

    const requests = [
      http.expectOne(urlIs(SUMMARY_URL)),
      http.expectOne(urlIs(TIMESERIES_URL)),
      http.expectOne(urlIs(CONVERSATIONS_URL)),
    ];
    for (const request of requests) {
      expect(request.request.headers.get('X-Tenant-ID')).toBe('tenant-a');
    }
    expect(store.summary().status).toBe('loading');

    requests[0].flush({ data: SUMMARY_WIRE });
    requests[1].flush({ data: TIMESERIES_WIRE });
    requests[2].flush({
      data: [CONVERSATION_WIRE],
      pagination: { next_cursor: null, has_more: false },
    });

    expect(store.summary().status).toBe('ready');
    expect(store.summary().data?.conversationVolume).toBe(12);
    expect(store.timeseries().data?.days).toHaveLength(1);
    expect(store.recent().data?.[0].customer.displayName).toBe('Maya Chen');
    expect(store.channelsEmpty()).toBe(false);
    expect(store.trendEmpty()).toBe(false);
  });

  it('asks for the last 30 days and the newest conversations', () => {
    setup(['analytics.view', 'conversations.view']);

    const summary = http.expectOne(urlIs(SUMMARY_URL));
    const { from, to } = overviewRange();
    expect(summary.request.params.get('from')).toBe(from);
    expect(summary.request.params.get('to')).toBe(to);
    http.expectOne(urlIs(TIMESERIES_URL));
    const conversations = http.expectOne(urlIs(CONVERSATIONS_URL));
    expect(conversations.request.params.get('limit')).toBe(String(RECENT_CONVERSATIONS));
  });

  it('spans exactly OVERVIEW_DAYS inclusive days ending today', () => {
    const { from, to } = overviewRange(new Date(2026, 10, 16));
    expect(to).toBe('2026-11-16');
    expect(from).toBe('2026-10-18');
    const days = (Date.parse(to) - Date.parse(from)) / 86_400_000 + 1;
    expect(days).toBe(OVERVIEW_DAYS);
  });

  it('never asks the analytics endpoints without analytics.view (an agent or viewer)', () => {
    const store = setup(['overview.view', 'conversations.view']);

    expect(store.canViewAnalytics()).toBe(false);
    expect(store.canViewConversations()).toBe(true);
    http.expectNone(urlIs(SUMMARY_URL));
    http.expectNone(urlIs(TIMESERIES_URL));
    http.expectOne(urlIs(CONVERSATIONS_URL)).flush({
      data: [],
      pagination: { next_cursor: null, has_more: false },
    });
    expect(store.summary().status).toBe('idle');
    expect(store.recent().status).toBe('ready');
  });

  it('never asks the inbox without conversations.view (finance or sales staff)', () => {
    const store = setup(['overview.view', 'analytics.view']);

    expect(store.canViewConversations()).toBe(false);
    http.expectNone(urlIs(CONVERSATIONS_URL));
    http.expectOne(urlIs(SUMMARY_URL)).flush({ data: SUMMARY_WIRE });
    http.expectOne(urlIs(TIMESERIES_URL)).flush({ data: TIMESERIES_WIRE });
    expect(store.recent().status).toBe('idle');
  });

  it('asks nothing when neither permission is held', () => {
    const store = setup(['overview.view']);

    expect(store.canViewAnalytics()).toBe(false);
    expect(store.canViewConversations()).toBe(false);
    http.expectNone(() => true);
  });

  it('keeps one failing section from blanking the others', () => {
    const store = setup(['analytics.view', 'conversations.view']);

    http.expectOne(urlIs(SUMMARY_URL)).flush({ data: SUMMARY_WIRE });
    http
      .expectOne(urlIs(TIMESERIES_URL))
      .flush({ code: 'internal_error' }, { status: 500, statusText: 'Server Error' });
    http.expectOne(urlIs(CONVERSATIONS_URL)).flush({
      data: [],
      pagination: { next_cursor: null, has_more: false },
    });

    expect(store.summary().status).toBe('ready');
    expect(store.timeseries().status).toBe('error');
    expect(store.timeseries().error?.status).toBe(500);
    expect(store.recent().status).toBe('ready');
  });

  it('retries only the failed section', () => {
    const store = setup(['analytics.view', 'conversations.view']);
    http.expectOne(urlIs(SUMMARY_URL)).flush({ data: SUMMARY_WIRE });
    http.expectOne(urlIs(TIMESERIES_URL)).flush({}, { status: 500, statusText: 'Server Error' });
    http.expectOne(urlIs(CONVERSATIONS_URL)).flush({
      data: [],
      pagination: { next_cursor: null, has_more: false },
    });

    store.retryTimeseries();

    expect(store.timeseries().status).toBe('loading');
    http.expectOne(urlIs(TIMESERIES_URL)).flush({ data: TIMESERIES_WIRE });
    expect(store.timeseries().status).toBe('ready');
    http.expectNone(urlIs(SUMMARY_URL));
    http.expectNone(urlIs(CONVERSATIONS_URL));
  });

  it('reports an empty trend and an empty channel list honestly', () => {
    const store = setup(['analytics.view']);
    http
      .expectOne(urlIs(SUMMARY_URL))
      .flush({ data: { ...SUMMARY_WIRE, conversation_volume: 0, channels: [] } });
    http.expectOne(urlIs(TIMESERIES_URL)).flush({
      data: {
        ...TIMESERIES_WIRE,
        days: [{ ...TIMESERIES_WIRE.days[0], conversation_volume: 0 }],
      },
    });

    expect(store.channelsEmpty()).toBe(true);
    expect(store.trendEmpty()).toBe(true);
  });

  it('drops the old workspace and loads the new one on a tenant switch', () => {
    const store = setup(['analytics.view', 'conversations.view'], 'tenant-a');
    http.expectOne(urlIs(SUMMARY_URL)).flush({ data: SUMMARY_WIRE });
    http.expectOne(urlIs(TIMESERIES_URL)).flush({ data: TIMESERIES_WIRE });
    http.expectOne(urlIs(CONVERSATIONS_URL)).flush({
      data: [CONVERSATION_WIRE],
      pagination: { next_cursor: null, has_more: false },
    });

    mockStore.setState(tenantState('tenant-b'));

    // Tenant A is gone at once, before tenant B answers.
    expect(store.recent().data).toBeNull();
    const requests = [
      http.expectOne(urlIs(SUMMARY_URL)),
      http.expectOne(urlIs(TIMESERIES_URL)),
      http.expectOne(urlIs(CONVERSATIONS_URL)),
    ];
    for (const request of requests) {
      expect(request.request.headers.get('X-Tenant-ID')).toBe('tenant-b');
    }
    requests.forEach((r) => r.cancelled || r.flush({ data: [] }));
  });
});
