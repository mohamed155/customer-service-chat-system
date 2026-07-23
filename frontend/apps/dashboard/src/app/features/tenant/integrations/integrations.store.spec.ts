import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { IntegrationList, IntegrationListItem } from '../../../core/api/tenant-api.models';
import { IntegrationsApiService } from './integrations-api.service';
import { IntegrationsStore } from './integrations.store';

const MOCK_ITEMS: IntegrationListItem[] = [
  {
    slug: 'generic-webhook',
    name: 'Generic Webhook',
    description: 'Forward events to a configurable HTTPS endpoint.',
    category: 'automation',
    isAvailable: true,
    status: 'not_connected',
  },
  {
    slug: 'slack',
    name: 'Slack',
    description: 'Send escalation summaries to team channels.',
    category: 'messaging',
    isAvailable: true,
    status: 'connected',
  },
];

const MOCK_LIST: IntegrationList = { items: MOCK_ITEMS };

const MOCK_RESPONSE: ApiResponse<IntegrationList> = { data: MOCK_LIST };

describe('IntegrationsStore', () => {
  const list = vi.fn();

  beforeEach(() => {
    list.mockReset();
    list.mockReturnValue(of(MOCK_RESPONSE));
    TestBed.configureTestingModule({
      providers: [IntegrationsStore, { provide: IntegrationsApiService, useValue: { list } }],
    });
  });

  it('load() populates items and clears loading', async () => {
    list.mockReturnValue(of(MOCK_RESPONSE));
    const store = TestBed.inject(IntegrationsStore);
    store.load();
    await vi.waitFor(() => {
      expect(store.items()).toEqual(MOCK_ITEMS);
      expect(store.loading()).toBe(false);
    });
  });

  it('API error sets error and clears loading', async () => {
    list.mockReturnValue(throwError(() => new Error('boom')));
    const store = TestBed.inject(IntegrationsStore);
    store.load();
    await vi.waitFor(() => {
      expect(store.error()).toBe('boom');
      expect(store.loading()).toBe(false);
    });
  });
});
