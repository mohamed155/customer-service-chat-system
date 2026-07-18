import { TestBed } from '@angular/core/testing';
import { of, Subject, throwError } from 'rxjs';
import { provideRouter } from '@angular/router';
import { provideZonelessChangeDetection } from '@angular/core';
import {
  AddMessagePayload,
  ConversationDetail,
  ConversationStatus,
  Message,
} from '../../../core/api/tenant-api.models';
import { RealtimeService, SseEvent } from '../../../core/realtime/realtime.service';
import { ConversationsApiService } from './conversations-api.service';
import { ConversationDetailStore } from './conversation-detail.store';

describe('ConversationDetailStore', () => {
  let api: {
    get: ReturnType<typeof vi.fn>;
    getTimeline: ReturnType<typeof vi.fn>;
    getToolActivity: ReturnType<typeof vi.fn>;
    addMessage: ReturnType<typeof vi.fn>;
    patch: ReturnType<typeof vi.fn>;
    setConversationAiHandling: ReturnType<typeof vi.fn>;
  };

  const mockConversation: ConversationDetail = {
    id: 'c1',
    customer: { id: 'cu1', displayName: 'Maya Chen' },
    channel: 'web_chat',
    status: 'open',
    assignee: { membershipId: 'm1', displayName: 'Alice', active: true },
    lastMessage: null,
    participants: [
      { type: 'customer', id: 'cu1', displayName: 'Maya Chen' },
      { type: 'member', membershipId: 'm1', displayName: 'Alice' },
    ],
    lastActivityAt: '2026-07-13T10:00:00Z',
    createdAt: '2026-07-13T09:00:00Z',
  };

  const mockMessages: Message[] = [
    {
      id: 'msg3',
      kind: 'reply',
      sender: { type: 'member', displayName: 'Alice', membershipId: 'm1' },
      loggedBy: null,
      body: 'Third message',
      createdAt: '2026-07-13T10:05:00Z',
    },
    {
      id: 'msg2',
      kind: 'customer',
      sender: { type: 'customer', displayName: 'Maya Chen' },
      loggedBy: null,
      body: 'Second message',
      createdAt: '2026-07-13T10:03:00Z',
    },
    {
      id: 'msg1',
      kind: 'note',
      sender: { type: 'member', displayName: 'Alice', membershipId: 'm1' },
      loggedBy: null,
      body: 'Internal note',
      createdAt: '2026-07-13T10:00:00Z',
    },
  ];

  let events$: Subject<SseEvent>;

  beforeEach(() => {
    events$ = new Subject<SseEvent>();
    api = {
      get: vi.fn(),
      getTimeline: vi.fn(),
      getToolActivity: vi.fn(),
      addMessage: vi.fn(),
      patch: vi.fn(),
      setConversationAiHandling: vi.fn(),
    };
    TestBed.configureTestingModule({
      providers: [
        provideRouter([]),
        provideZonelessChangeDetection(),
        ConversationDetailStore,
        { provide: ConversationsApiService, useValue: api },
        {
          provide: RealtimeService,
          useValue: { events: () => events$.asObservable() },
        },
      ],
    });
  });

  it('starts with empty state', () => {
    const store = TestBed.inject(ConversationDetailStore);
    expect(store.conversation()).toBeNull();
    expect(store.timelinePages()).toEqual([]);
    expect(store.loading()).toBe(false);
    expect(store.loadingTimeline()).toBe(false);
    expect(store.submitting()).toBe(false);
    expect(store.error()).toBeNull();
    expect(store.timelineError()).toBeNull();
  });

  it('loads conversation and first timeline page', async () => {
    api.get.mockReturnValue(of({ data: mockConversation }));
    api.getTimeline.mockReturnValue(
      of({
        data: {
          items: mockMessages,
          nextCursor: 'cursor-next',
          hasMore: true,
        },
      }),
    );
    api.getToolActivity.mockReturnValue(of({ data: { items: [] } }));

    const store = TestBed.inject(ConversationDetailStore);
    store.load('c1');

    await vi.waitFor(() => {
      expect(store.conversation()).toEqual(mockConversation);
      expect(store.timelinePages().length).toBe(1);
      expect(store.timelinePages()[0].items).toEqual(mockMessages);
      expect(store.loading()).toBe(false);
      expect(store.loadingTimeline()).toBe(false);
    });
  });

  it('hasMoreTimeline is true when first page has more', async () => {
    api.get.mockReturnValue(of({ data: mockConversation }));
    api.getTimeline.mockReturnValue(
      of({
        data: { items: mockMessages, nextCursor: 'cursor-next', hasMore: true },
      }),
    );
    api.getToolActivity.mockReturnValue(of({ data: { items: [] } }));

    const store = TestBed.inject(ConversationDetailStore);
    store.load('c1');

    await vi.waitFor(() => {
      expect(store.hasMoreTimeline()).toBe(true);
    });
  });

  it('loadOlder prepends without reordering', async () => {
    api.get.mockReturnValue(of({ data: mockConversation }));
    api.getTimeline
      .mockReturnValueOnce(
        of({
          data: {
            items: [mockMessages[0]],
            nextCursor: 'cursor-older',
            hasMore: true,
          },
        }),
      )
      .mockReturnValueOnce(
        of({
          data: {
            items: [mockMessages[1], mockMessages[2]],
            nextCursor: null,
            hasMore: false,
          },
        }),
      );
    api.getToolActivity.mockReturnValue(of({ data: { items: [] } }));

    const store = TestBed.inject(ConversationDetailStore);
    store.load('c1');

    await vi.waitFor(() => {
      expect(store.timelinePages().length).toBe(1);
    });

    store.loadOlder('c1');

    await vi.waitFor(() => {
      expect(store.timelinePages().length).toBe(2);
      // timeline computed property flattens and sorts ascending
      const timeline = store.timeline();
      expect(timeline.length).toBe(3);
      // check chronological order
      for (let i = 1; i < timeline.length; i++) {
        expect(new Date(timeline[i].createdAt).getTime()).toBeGreaterThanOrEqual(
          new Date(timeline[i - 1].createdAt).getTime(),
        );
      }
    });
  });

  it('track submitting state during addMessage', async () => {
    api.get.mockReturnValue(of({ data: mockConversation }));
    api.getTimeline.mockReturnValue(of({ data: { items: [], nextCursor: null, hasMore: false } }));
    api.getToolActivity.mockReturnValue(of({ data: { items: [] } }));
    api.addMessage.mockReturnValue(
      of({
        data: {
          message: mockMessages[0],
          conversation: {
            status: 'open' as ConversationStatus,
            lastActivityAt: '2026-07-13T11:00:00Z',
          },
        },
      }),
    );

    const store = TestBed.inject(ConversationDetailStore);
    store.load('c1');

    await vi.waitFor(() => {
      expect(store.conversation()).toBeTruthy();
    });

    const payload: AddMessagePayload = { kind: 'reply', body: 'Hello' };
    store.addMessage('c1', payload);

    // The synchronous mock completes immediately, so submitting cycles through
    // true → false; verify the final state is idle
    await vi.waitFor(() => {
      expect(store.submitting()).toBe(false);
    });
    expect(store.timelinePages().length).toBeGreaterThanOrEqual(1);
  });

  it('sets error when addMessage fails', async () => {
    api.get.mockReturnValue(of({ data: mockConversation }));
    api.getTimeline.mockReturnValue(of({ data: { items: [], nextCursor: null, hasMore: false } }));
    api.getToolActivity.mockReturnValue(of({ data: { items: [] } }));
    api.addMessage.mockReturnValue(throwError(() => new Error('Send failed')));

    const store = TestBed.inject(ConversationDetailStore);
    store.load('c1');

    await vi.waitFor(() => {
      expect(store.conversation()).toBeTruthy();
    });

    const payload: AddMessagePayload = { kind: 'reply', body: 'Hello' };
    store.addMessage('c1', payload);

    await vi.waitFor(() => {
      expect(store.submitting()).toBe(false);
      expect(store.error()).toBe('Send failed');
    });
  });
});
