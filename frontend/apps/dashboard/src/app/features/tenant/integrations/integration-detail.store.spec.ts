import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { IntegrationDetail, IntegrationEventList } from '../../../core/api/tenant-api.models';
import { IntegrationsApiService } from './integrations-api.service';
import { IntegrationDetailStore } from './integration-detail.store';

const MOCK_DETAIL: IntegrationDetail = {
  slug: 'generic-webhook',
  name: 'Generic Webhook',
  description: 'Forward events to a configurable HTTPS endpoint.',
  category: 'automation',
  isAvailable: true,
  status: 'not_connected',
  configSchema: [
    { key: 'url', label: 'Endpoint URL', kind: 'text', required: true },
    { key: 'signing_secret', label: 'Signing Secret', kind: 'secret', required: true },
  ],
  connection: null,
};

const MOCK_CONNECTED: IntegrationDetail = {
  ...MOCK_DETAIL,
  status: 'connected',
  connection: {
    config: { url: 'https://example.com/hook' },
    secrets: [{ fieldKey: 'signing_secret', hint: 'XYZ' }],
    webhookUrl: 'https://api.example.com/hooks/v1/abc',
    connectedAt: '2026-07-22T10:00:00Z',
    disconnectedAt: null,
  },
};

const MOCK_RESPONSE: ApiResponse<IntegrationDetail> = { data: MOCK_DETAIL };
const MOCK_CONNECTED_RESPONSE: ApiResponse<IntegrationDetail> = { data: MOCK_CONNECTED };

const MOCK_EVENTS_PAGE_1: IntegrationEventList = {
  data: [
    {
      id: 'evt_1',
      eventType: 'connected',
      outcome: null,
      reason: null,
      actorMembershipId: 'mem_1',
      createdAt: '2026-07-22T10:00:00Z',
    },
    {
      id: 'evt_2',
      eventType: 'delivery_rejected',
      outcome: 'failure',
      reason: 'invalid_signature',
      actorMembershipId: null,
      createdAt: '2026-07-22T10:05:00Z',
    },
  ],
  pagination: { nextCursor: 'cursor_page2', hasMore: true },
};

const MOCK_EVENTS_PAGE_2: IntegrationEventList = {
  data: [
    {
      id: 'evt_3',
      eventType: 'delivery_accepted',
      outcome: 'success',
      reason: null,
      actorMembershipId: null,
      createdAt: '2026-07-22T10:10:00Z',
    },
  ],
  pagination: { nextCursor: null, hasMore: false },
};

const MOCK_EVENTS_RESPONSE_PAGE_1: ApiResponse<IntegrationEventList> = {
  data: MOCK_EVENTS_PAGE_1,
};
const MOCK_EVENTS_RESPONSE_PAGE_2: ApiResponse<IntegrationEventList> = {
  data: MOCK_EVENTS_PAGE_2,
};

describe('IntegrationDetailStore', () => {
  const detail = vi.fn();
  const connect = vi.fn();
  const updateConfig = vi.fn();
  const disconnect = vi.fn();
  const events = vi.fn();

  beforeEach(() => {
    detail.mockReset();
    connect.mockReset();
    updateConfig.mockReset();
    disconnect.mockReset();
    events.mockReset();
    TestBed.configureTestingModule({
      providers: [
        IntegrationDetailStore,
        {
          provide: IntegrationsApiService,
          useValue: { detail, connect, updateConfig, disconnect, events },
        },
      ],
    });
  });

  it('load(slug) populates detail and clears loading', async () => {
    detail.mockReturnValue(of(MOCK_RESPONSE));
    const store = TestBed.inject(IntegrationDetailStore);
    store.load('generic-webhook');
    await vi.waitFor(() => {
      expect(store.detail()).toEqual(MOCK_DETAIL);
      expect(store.loading()).toBe(false);
    });
  });

  it('load API error sets error and clears loading', async () => {
    detail.mockReturnValue(throwError(() => new Error('boom')));
    const store = TestBed.inject(IntegrationDetailStore);
    store.load('generic-webhook');
    await vi.waitFor(() => {
      expect(store.error()).toBe('boom');
      expect(store.loading()).toBe(false);
    });
  });

  it('load(slug) resets the events sub-state', async () => {
    detail.mockReturnValue(of(MOCK_RESPONSE));
    events.mockReturnValue(of(MOCK_EVENTS_RESPONSE_PAGE_1));
    const store = TestBed.inject(IntegrationDetailStore);
    store.loadFirstPageEvents('generic-webhook');
    await vi.waitFor(() => expect(store.events().length).toBe(2));
    store.load('generic-webhook');
    await vi.waitFor(() => {
      expect(store.events()).toEqual([]);
      expect(store.eventsCursor()).toBeNull();
      expect(store.eventsHasMore()).toBe(false);
    });
  });

  it('connect patches detail from response and clears saving', async () => {
    connect.mockReturnValue(of(MOCK_CONNECTED_RESPONSE));
    const store = TestBed.inject(IntegrationDetailStore);
    store.connect('generic-webhook', {
      config: { url: 'https://example.com/hook' },
      secrets: { signing_secret: 'whsec_abc123' },
    });
    await vi.waitFor(() => {
      expect(store.detail()).toEqual(MOCK_CONNECTED);
      expect(store.saving()).toBe(false);
      expect(store.error()).toBeNull();
    });
  });

  it('connect error sets error and clears saving', async () => {
    connect.mockReturnValue(throwError(() => new Error('connect failed')));
    const store = TestBed.inject(IntegrationDetailStore);
    store.connect('generic-webhook', {
      config: { url: 'https://example.com/hook' },
      secrets: { signing_secret: 'whsec_abc123' },
    });
    await vi.waitFor(() => {
      expect(store.error()).toBe('connect failed');
      expect(store.saving()).toBe(false);
    });
  });

  it('disconnect patches detail and clears saving', async () => {
    disconnect.mockReturnValue(of(MOCK_RESPONSE));
    const store = TestBed.inject(IntegrationDetailStore);
    store.disconnect('generic-webhook');
    await vi.waitFor(() => {
      expect(store.detail()).toEqual(MOCK_DETAIL);
      expect(store.saving()).toBe(false);
      expect(store.error()).toBeNull();
    });
  });

  it('loadFirstPageEvents(slug) fetches with null cursor and stores result', async () => {
    events.mockReturnValue(of(MOCK_EVENTS_RESPONSE_PAGE_1));
    const store = TestBed.inject(IntegrationDetailStore);
    store.loadFirstPageEvents('generic-webhook');
    await vi.waitFor(() => {
      expect(events).toHaveBeenCalledWith('generic-webhook', null);
      expect(store.events().length).toBe(2);
      expect(store.eventsCursor()).toBe('cursor_page2');
      expect(store.eventsHasMore()).toBe(true);
      expect(store.eventsLoading()).toBe(false);
      expect(store.eventsError()).toBeNull();
    });
  });

  it('loadFirstPageEvents(slug) error sets eventsError and clears loading', async () => {
    events.mockReturnValue(throwError(() => new Error('events boom')));
    const store = TestBed.inject(IntegrationDetailStore);
    store.loadFirstPageEvents('generic-webhook');
    await vi.waitFor(() => {
      expect(store.eventsError()).toBe('events boom');
      expect(store.eventsLoading()).toBe(false);
    });
  });

  it('loadMoreEvents(slug) appends using current cursor and advances it', async () => {
    events.mockReturnValueOnce(of(MOCK_EVENTS_RESPONSE_PAGE_1));
    const store = TestBed.inject(IntegrationDetailStore);
    store.loadFirstPageEvents('generic-webhook');
    await vi.waitFor(() => expect(store.events().length).toBe(2));

    events.mockReturnValueOnce(of(MOCK_EVENTS_RESPONSE_PAGE_2));
    store.loadMoreEvents('generic-webhook');
    await vi.waitFor(() => {
      expect(events).toHaveBeenLastCalledWith('generic-webhook', 'cursor_page2');
      expect(store.events().length).toBe(3);
      expect(store.events()[0].id).toBe('evt_1');
      expect(store.events()[2].id).toBe('evt_3');
      expect(store.eventsCursor()).toBeNull();
      expect(store.eventsHasMore()).toBe(false);
      expect(store.eventsLoading()).toBe(false);
    });
  });

  it('loadMoreEvents(slug) is a no-op when there is no cursor', async () => {
    events.mockReset();
    events.mockReturnValue(of(MOCK_EVENTS_RESPONSE_PAGE_2));
    const store = TestBed.inject(IntegrationDetailStore);
    store.loadMoreEvents('generic-webhook');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(events).not.toHaveBeenCalled();
    expect(store.eventsLoading()).toBe(false);
  });
});
