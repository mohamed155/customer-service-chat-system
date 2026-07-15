import { TestBed } from '@angular/core/testing';
import { of, Subject } from 'rxjs';
import { provideZonelessChangeDetection } from '@angular/core';
import { SseEvent } from '../../../core/realtime/realtime.service';
import { RealtimeService } from '../../../core/realtime/realtime.service';
import { Escalation, QueueEntry } from '../../../core/api/tenant-api.models';
import { EscalationsApiService } from './escalations-api.service';
import { EscalationQueueStore } from './escalation-queue.store';

describe('EscalationQueueStore', () => {
  let mockApi: { listQueue: ReturnType<typeof vi.fn>; claim: ReturnType<typeof vi.fn> };
  let eventsSubject: Subject<SseEvent>;
  const mockEscalation: Escalation = {
    id: 'e-1',
    conversationId: 'c-1',
    reason: 'customer_requested',
    requiredSkills: [],
    status: 'queued',
    routing: null,
    escalatedAt: '2026-07-14T10:00:00Z',
    closedAt: null,
  };
  const mockEntry: QueueEntry = {
    escalation: mockEscalation,
    conversation: { id: 'c-1', channel: 'web_chat', customer: { id: 'cu-1', name: 'Maya Chen' } },
    waitingSeconds: 120,
  };

  function configureStore() {
    TestBed.configureTestingModule({
      providers: [
        provideZonelessChangeDetection(),
        EscalationQueueStore,
        { provide: EscalationsApiService, useValue: mockApi },
        { provide: RealtimeService, useValue: { events: () => eventsSubject.asObservable() } },
      ],
    });
    return TestBed.inject(EscalationQueueStore);
  }

  beforeEach(() => {
    mockApi = { listQueue: vi.fn(), claim: vi.fn() };
    eventsSubject = new Subject<SseEvent>();
  });

  it('loads first page on init', () => {
    mockApi.listQueue.mockReturnValue(
      of({ data: { items: [mockEntry], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    expect(store.items()).toEqual([mockEntry]);
    expect(store.loading()).toBe(false);
  });

  it('removes item optimistically on claim and rolls back on 409 with message', () => {
    const secondEntry: QueueEntry = {
      escalation: { ...mockEscalation, id: 'e-2' },
      conversation: { id: 'c-2', channel: 'email', customer: { id: 'cu-2', name: 'Jon Bell' } },
      waitingSeconds: 60,
    };
    mockApi.listQueue.mockReturnValue(
      of({ data: { items: [mockEntry, secondEntry], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    expect(store.items().length).toBe(2);

    const claimSubject = new Subject<{ data: undefined }>();
    mockApi.claim.mockReturnValue(claimSubject);

    store.claim('e-1');
    expect(store.items().map((i) => i.escalation.id)).toEqual(['e-2']);
    expect(store.error()).toBeNull();

    claimSubject.error({ code: 'conflict', message: 'Already claimed', status: 409 });
    expect(store.items().map((i) => i.escalation.id)).toEqual(['e-1', 'e-2']);
    expect(store.error()).toContain('already claimed by another agent');
  });

  it('applies escalation.queued realtime event without refetch', () => {
    mockApi.listQueue.mockReturnValue(
      of({ data: { items: [mockEntry], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    expect(store.items().length).toBe(1);

    mockApi.listQueue.mockClear();
    eventsSubject.next({ event: 'escalation.queued', id: '1', data: '{}' });

    expect(mockApi.listQueue).toHaveBeenCalled();
  });

  it('removes escalation.assigned and escalation.removed items without refetch', () => {
    const entry2: QueueEntry = {
      escalation: { ...mockEscalation, id: 'e-2' },
      conversation: { id: 'c-2', channel: 'email', customer: { id: 'cu-2', name: 'Jon' } },
      waitingSeconds: 30,
    };
    mockApi.listQueue.mockReturnValue(
      of({ data: { items: [mockEntry, entry2], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    expect(store.items().length).toBe(2);

    mockApi.listQueue.mockClear();
    eventsSubject.next({
      event: 'escalation.assigned',
      id: '2',
      data: JSON.stringify({ escalationId: 'e-1' }),
    });

    expect(store.items().map((i) => i.escalation.id)).toEqual(['e-2']);
    expect(mockApi.listQueue).not.toHaveBeenCalled();

    eventsSubject.next({
      event: 'escalation.removed',
      id: '3',
      data: JSON.stringify({ escalationId: 'e-2', cause: 'closed' }),
    });
    expect(store.items().length).toBe(0);
  });
});
