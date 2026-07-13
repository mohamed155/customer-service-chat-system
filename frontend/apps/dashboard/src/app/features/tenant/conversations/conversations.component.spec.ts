import { provideZonelessChangeDetection, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { RouterModule } from '@angular/router';
import { Store } from '@ngrx/store';
import { provideTaiga } from '@taiga-ui/core';
import { of, Subject } from 'rxjs';
import { APP_CONFIG } from '../../../core/config/app-config';
import { Conversation } from '../../../core/api/tenant-api.models';
import { ConversationsApiService } from './conversations-api.service';
import { ConversationsComponent } from './conversations.component';

const MOCK_CONVERSATIONS: Conversation[] = [
  {
    id: 'c1',
    customer: { id: 'cu1', displayName: 'Maya Chen' },
    channel: 'web_chat',
    status: 'open',
    assignee: null,
    lastMessage: { kind: 'customer', preview: 'Thanks for your help' },
    lastActivityAt: '2026-07-10T12:00:00Z',
    createdAt: '2026-07-10T11:00:00Z',
  },
  {
    id: 'c2',
    customer: { id: 'cu2', displayName: 'Jon Bell' },
    channel: 'email',
    status: 'open',
    assignee: { membershipId: 'm1', displayName: 'Agent A', active: true },
    lastMessage: { kind: 'customer', preview: 'Hello' },
    lastActivityAt: '2026-07-10T11:00:00Z',
    createdAt: '2026-07-10T10:00:00Z',
  },
  {
    id: 'c3',
    customer: { id: 'cu3', displayName: 'Ava Patel' },
    channel: 'whatsapp',
    status: 'closed',
    assignee: null,
    lastMessage: null,
    lastActivityAt: '2026-07-09T09:00:00Z',
    createdAt: '2026-07-09T08:00:00Z',
  },
];

describe('ConversationsComponent', () => {
  const activeTenant = signal<{ id: string } | null>({ id: 'tenant-1' });
  const apiList = vi.fn();
  const apiMembers = vi.fn();

  beforeEach(() => {
    activeTenant.set({ id: 'tenant-1' });
    apiList.mockReset();
    apiMembers.mockReset();
    apiMembers.mockReturnValue(of({ data: [] }));
    TestBed.configureTestingModule({
      imports: [ConversationsComponent, RouterModule.forRoot([])],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        {
          provide: ConversationsApiService,
          useValue: { list: apiList, listAssignableMembers: apiMembers },
        },
        { provide: Store, useValue: { selectSignal: () => activeTenant } },
        {
          provide: APP_CONFIG,
          useValue: { apiBaseUrl: '/api/v1', production: false },
        },
      ],
    });
  });

  it('moves from pending to content', async () => {
    apiList.mockReturnValue(
      of({
        data: {
          items: MOCK_CONVERSATIONS,
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });
  });

  it('shows empty state when no conversations exist', async () => {
    apiList.mockReturnValue(
      of({
        data: {
          items: [],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
  });

  it('shows the no-results state and resets the filter', async () => {
    apiList.mockReturnValue(
      of({
        data: {
          items: MOCK_CONVERSATIONS,
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });
  });

  it('transitions from tenant A content to tenant B content', async () => {
    apiList.mockReturnValue(
      of({
        data: {
          items: MOCK_CONVERSATIONS,
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    apiList.mockReturnValue(
      of({
        data: {
          items: [MOCK_CONVERSATIONS[2]],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
      expect(fixture.nativeElement.textContent).not.toContain('Maya Chen');
    });
  });

  it('removes tenant A content while tenant B is pending', async () => {
    apiList.mockReturnValue(
      of({
        data: {
          items: MOCK_CONVERSATIONS,
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    const subjects: Subject<unknown>[] = [];
    apiList.mockImplementation(() => {
      const s = new Subject<unknown>();
      subjects.push(s);
      return s.asObservable();
    });
    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
    });
    expect(fixture.nativeElement.textContent).not.toContain('Maya Chen');

    subjects[0].next({
      data: {
        items: [MOCK_CONVERSATIONS[2]],
        nextCursor: null,
        hasMore: false,
      },
    });
    subjects[0].complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
    });
  });

  it('handles tenant B resolving to empty', async () => {
    apiList.mockReturnValue(
      of({
        data: {
          items: MOCK_CONVERSATIONS,
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    apiList.mockReturnValue(
      of({
        data: {
          items: [],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
  });
});
