import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of } from 'rxjs';
import { ApiService } from '../../../core/api/api.service';
import { conversationFromWire, ConversationWire } from '../../../core/api/tenant-api.models';
import { OverviewApiService } from './overview-api.service';

const WIRE: ConversationWire = {
  id: 'c1',
  customer: { id: 'cu1', display_name: 'Maya Chen' },
  channel: 'web_chat',
  status: 'open',
  assignee: null,
  last_message: { kind: 'customer', preview: 'Hello' },
  last_activity_at: '2026-11-16T09:00:00Z',
  created_at: '2026-11-16T08:00:00Z',
};

describe('OverviewApiService', () => {
  const get = vi.fn();

  beforeEach(() => {
    get.mockReset();
    TestBed.configureTestingModule({
      providers: [OverviewApiService, { provide: ApiService, useValue: { get } }],
    });
  });

  it('asks for the newest conversations with a limit and maps the wire shape', async () => {
    get.mockReturnValue(
      of({
        data: { data: [WIRE], pagination: { next_cursor: null, has_more: false } },
        requestId: 'req-1',
      }),
    );

    const result = await firstValueFrom(TestBed.inject(OverviewApiService).recentConversations(5));

    const [url, params] = get.mock.calls[0];
    expect(url).toBe('/tenant/conversations');
    expect(params.get('limit')).toBe('5');
    expect(result.data).toEqual([conversationFromWire(WIRE)]);
    expect(result.requestId).toBe('req-1');
  });
});
