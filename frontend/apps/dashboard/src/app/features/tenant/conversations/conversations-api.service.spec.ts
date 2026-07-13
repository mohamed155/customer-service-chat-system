import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of } from 'rxjs';
import { ApiService } from '../../../core/api/api.service';
import {
  AddMessagePayload,
  ConversationDetailWire,
  ConversationListQuery,
  conversationDetailFromWire,
  conversationFromWire,
  ConversationWire,
  CreateConversationPayload,
  MessageWire,
  messageFromWire,
  PatchConversationPayload,
} from '../../../core/api/tenant-api.models';
import { ConversationsApiService } from './conversations-api.service';

describe('ConversationsApiService', () => {
  let service: ConversationsApiService;
  let api: {
    get: ReturnType<typeof vi.fn>;
    post: ReturnType<typeof vi.fn>;
    patch: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    api = { get: vi.fn(), post: vi.fn(), patch: vi.fn() };
    TestBed.configureTestingModule({
      providers: [ConversationsApiService, { provide: ApiService, useValue: api }],
    });
    service = TestBed.inject(ConversationsApiService);
  });

  describe('list', () => {
    it('passes query params and maps wire response', async () => {
      const wire: ConversationWire = {
        id: 'c1',
        customer: { id: 'cu1', display_name: 'Maya Chen' },
        channel: 'web_chat',
        status: 'open',
        assignee: null,
        last_message: null,
        last_activity_at: '2026-07-13T10:00:00Z',
        created_at: '2026-07-13T09:00:00Z',
      };
      const response = {
        data: { data: [wire], pagination: { next_cursor: 'cursor-abc', has_more: true } },
        requestId: 'req-1',
      };
      api.get.mockReturnValue(of(response));

      const query: ConversationListQuery = { status: 'open', limit: 20 };
      const result = await firstValueFrom(service.list(query));

      expect(api.get).toHaveBeenCalled();
      const [url, params] = api.get.mock.calls[0];
      expect(url).toBe('/tenant/conversations');
      expect(params.toString()).toContain('status=open');
      expect(params.toString()).toContain('limit=20');
      expect(result.data).toEqual({
        items: [conversationFromWire(wire)],
        nextCursor: 'cursor-abc',
        hasMore: true,
      });
      expect(result.requestId).toBe('req-1');
    });

    it('passes cursor through to the API', async () => {
      const wire: ConversationWire = {
        id: 'c2',
        customer: { id: 'cu2', display_name: 'Jon Bell' },
        channel: 'email',
        status: 'open',
        assignee: null,
        last_message: null,
        last_activity_at: '2026-07-13T09:00:00Z',
        created_at: '2026-07-13T08:00:00Z',
      };
      api.get.mockReturnValue(
        of({
          data: { data: [wire], pagination: { next_cursor: null, has_more: false } },
        }),
      );

      await firstValueFrom(service.list({ cursor: 'cursor-prev' }));

      const params = api.get.mock.calls[0][1];
      expect(params.toString()).toContain('cursor=cursor-prev');
    });
  });

  describe('listAssignableMembers', () => {
    it('calls GET /tenant/members', async () => {
      const members = [{ id: 'm1', displayName: 'Alice' }];
      api.get.mockReturnValue(of({ data: members }));

      const result = await firstValueFrom(service.listAssignableMembers());

      expect(api.get).toHaveBeenCalledWith('/tenant/members');
      expect(result.data).toEqual(members);
    });
  });

  describe('get', () => {
    it('fetches conversation detail by id and maps from wire', async () => {
      const wire: ConversationDetailWire = {
        id: 'c1',
        customer: { id: 'cu1', display_name: 'Maya Chen' },
        channel: 'web_chat',
        status: 'open',
        assignee: { membership_id: 'm1', display_name: 'Alice', active: true },
        last_message: null,
        last_activity_at: '2026-07-13T10:00:00Z',
        created_at: '2026-07-13T09:00:00Z',
        participants: [{ type: 'customer', id: 'cu1', display_name: 'Maya Chen' }],
      };
      api.get.mockReturnValue(of({ data: { data: wire }, requestId: 'req-detail' }));

      const result = await firstValueFrom(service.get('c1'));

      expect(api.get).toHaveBeenCalledWith('/tenant/conversations/c1');
      expect(result.data).toEqual(conversationDetailFromWire(wire));
      expect(result.requestId).toBe('req-detail');
    });
  });

  describe('getTimeline', () => {
    it('fetches messages paginated without cursor', async () => {
      const msgWire: MessageWire = {
        id: 'msg1',
        kind: 'reply',
        sender: { type: 'member', display_name: 'Alice', membership_id: 'm1' },
        logged_by: null,
        body: 'Hello',
        created_at: '2026-07-13T10:00:00Z',
      };
      api.get.mockReturnValue(
        of({
          data: {
            data: [msgWire],
            pagination: { next_cursor: null, has_more: false },
          },
          requestId: 'req-tl',
        }),
      );

      const result = await firstValueFrom(service.getTimeline('c1'));

      expect(api.get).toHaveBeenCalledWith('/tenant/conversations/c1/messages', undefined);
      expect(result.data).toEqual({
        items: [messageFromWire(msgWire)],
        nextCursor: null,
        hasMore: false,
      });
    });

    it('passes cursor param when provided', async () => {
      api.get.mockReturnValue(
        of({
          data: { data: [], pagination: { next_cursor: null, has_more: false } },
        }),
      );

      await firstValueFrom(service.getTimeline('c1', 'cursor-abc'));

      const [url, params] = api.get.mock.calls[0];
      expect(url).toBe('/tenant/conversations/c1/messages');
      expect(params.toString()).toContain('cursor=cursor-abc');
    });
  });

  describe('addMessage', () => {
    it('sends message payload and maps response', async () => {
      const payload: AddMessagePayload = {
        kind: 'reply',
        body: 'Looking into this',
      };
      const msgWire: MessageWire = {
        id: 'msg-new',
        kind: 'reply',
        sender: { type: 'member', display_name: 'Alice', membership_id: 'm1' },
        logged_by: null,
        body: 'Looking into this',
        created_at: '2026-07-13T11:00:00Z',
      };
      api.post.mockReturnValue(
        of({
          data: {
            data: {
              message: msgWire,
              conversation: { status: 'open', last_activity_at: '2026-07-13T11:00:00Z' },
            },
          },
          requestId: 'req-msg',
        }),
      );

      const result = await firstValueFrom(service.addMessage('c1', payload));

      expect(api.post).toHaveBeenCalledWith('/tenant/conversations/c1/messages', {
        kind: 'reply',
        body: 'Looking into this',
      });
      expect(result.data.message).toEqual(messageFromWire(msgWire));
      expect(result.data.conversation.status).toBe('open');
      expect(result.data.conversation.lastActivityAt).toBe('2026-07-13T11:00:00Z');
    });
  });

  describe('patch', () => {
    it('sends status and assignment changes mapped to wire', async () => {
      const payload: PatchConversationPayload = {
        status: 'resolved',
      };
      const detailWire: ConversationDetailWire = {
        id: 'c1',
        customer: { id: 'cu1', display_name: 'Maya Chen' },
        channel: 'web_chat',
        status: 'resolved',
        assignee: null,
        last_message: null,
        last_activity_at: '2026-07-13T10:00:00Z',
        created_at: '2026-07-13T09:00:00Z',
        participants: [],
      };
      api.patch.mockReturnValue(of({ data: { data: detailWire }, requestId: 'req-patch' }));

      const result = await firstValueFrom(service.patch('c1', payload));

      expect(api.patch).toHaveBeenCalledWith('/tenant/conversations/c1', { status: 'resolved' });
      expect(result.data).toEqual(conversationDetailFromWire(detailWire));
    });

    it('sends null assigned_membership_id for unassignment', async () => {
      const payload: PatchConversationPayload = {
        assignedMembershipId: null,
      };
      const detailWire: ConversationDetailWire = {
        id: 'c1',
        customer: { id: 'cu1', display_name: 'Maya Chen' },
        channel: 'web_chat',
        status: 'open',
        assignee: null,
        last_message: null,
        last_activity_at: '2026-07-13T10:00:00Z',
        created_at: '2026-07-13T09:00:00Z',
        participants: [],
      };
      api.patch.mockReturnValue(of({ data: { data: detailWire } }));

      await firstValueFrom(service.patch('c1', payload));

      expect(api.patch).toHaveBeenCalledWith('/tenant/conversations/c1', {
        assigned_membership_id: null,
      });
    });
  });

  describe('create', () => {
    it('sends create payload and maps response', async () => {
      const payload: CreateConversationPayload = {
        customerId: 'cu1',
        channel: 'web_chat',
        message: { body: 'I need help' },
      };
      const detailWire: ConversationDetailWire = {
        id: 'c-new',
        customer: { id: 'cu1', display_name: 'Maya Chen' },
        channel: 'web_chat',
        status: 'open',
        assignee: null,
        last_message: null,
        last_activity_at: '2026-07-13T12:00:00Z',
        created_at: '2026-07-13T12:00:00Z',
        participants: [],
      };
      api.post.mockReturnValue(of({ data: { data: detailWire }, requestId: 'req-create' }));

      const result = await firstValueFrom(service.create(payload));

      expect(api.post).toHaveBeenCalledWith('/tenant/conversations', {
        customer_id: 'cu1',
        channel: 'web_chat',
        message: { body: 'I need help' },
      });
      expect(result.data).toEqual(conversationDetailFromWire(detailWire));
      expect(result.requestId).toBe('req-create');
    });
  });
});
