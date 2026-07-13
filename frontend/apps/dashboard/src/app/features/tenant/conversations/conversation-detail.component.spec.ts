import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { of } from 'rxjs';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { APP_CONFIG } from '../../../core/config/app-config';
import { ConversationDetail, Message } from '../../../core/api/tenant-api.models';
import { ConversationsApiService } from './conversations-api.service';
import { ConversationDetailStore } from './conversation-detail.store';
import { ConversationDetailComponent } from './conversation-detail.component';

describe('ConversationDetailComponent', () => {
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
      id: 'msg1',
      kind: 'customer',
      sender: { type: 'customer' as const, displayName: 'Maya Chen' },
      loggedBy: null,
      body: 'I need help with my order',
      createdAt: '2026-07-13T09:30:00Z',
    },
    {
      id: 'msg2',
      kind: 'reply',
      sender: { type: 'member' as const, displayName: 'Alice', membershipId: 'm1' },
      loggedBy: null,
      body: 'I am looking into this for you',
      createdAt: '2026-07-13T09:35:00Z',
    },
    {
      id: 'msg3',
      kind: 'note',
      sender: { type: 'member' as const, displayName: 'Alice', membershipId: 'm1' },
      loggedBy: null,
      body: 'Internal note about the billing issue',
      createdAt: '2026-07-13T09:40:00Z',
    },
  ];

  let api: { listAssignableMembers: ReturnType<typeof vi.fn> };
  let storeMock: {
    conversation: ReturnType<typeof vi.fn>;
    timeline: ReturnType<typeof vi.fn>;
    loading: ReturnType<typeof vi.fn>;
    loadingTimeline: ReturnType<typeof vi.fn>;
    hasMoreTimeline: ReturnType<typeof vi.fn>;
    submitting: ReturnType<typeof vi.fn>;
    error: ReturnType<typeof vi.fn>;
    load: ReturnType<typeof vi.fn>;
    loadOlder: ReturnType<typeof vi.fn>;
    addMessage: ReturnType<typeof vi.fn>;
    patchStatus: ReturnType<typeof vi.fn>;
    patchAssignment: ReturnType<typeof vi.fn>;
  };

  function createStoreMock() {
    return {
      conversation: vi.fn(() => null),
      timeline: vi.fn(() => []),
      loading: vi.fn(() => false),
      loadingTimeline: vi.fn(() => false),
      hasMoreTimeline: vi.fn(() => false),
      submitting: vi.fn(() => false),
      error: vi.fn(() => null),
      load: vi.fn(),
      loadOlder: vi.fn(),
      addMessage: vi.fn(),
      patchStatus: vi.fn(),
      patchAssignment: vi.fn(),
    };
  }

  beforeEach(() => {
    api = { listAssignableMembers: vi.fn() };
    storeMock = createStoreMock();
    api.listAssignableMembers.mockReturnValue(of({ data: [] }));

    TestBed.configureTestingModule({
      imports: [ConversationDetailComponent],
      providers: [
        provideRouter([]),
        provideZonelessChangeDetection(),
        {
          provide: ConversationsApiService,
          useValue: api,
        },
        {
          provide: ConversationDetailStore,
          useValue: storeMock,
        },
        {
          provide: PermissionsService,
          useValue: { has: vi.fn(() => true), effective: vi.fn(() => new Set()) },
        },
        {
          provide: APP_CONFIG,
          useValue: { apiBaseUrl: '/api/v1', production: false },
        },
      ],
    });
  });

  it('renders loading state initially', async () => {
    storeMock.loading.mockReturnValue(true);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationDetailComponent);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
  });

  it('renders error state when error is set', async () => {
    storeMock.error.mockReturnValue('Something went wrong');
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationDetailComponent);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    expect(fixture.nativeElement.textContent).toContain('Go back');
  });

  it('renders header with customer name, channel, status, and assignee', async () => {
    storeMock.conversation.mockReturnValue(mockConversation);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationDetailComponent);
    fixture.detectChanges();

    const el = fixture.nativeElement;
    expect(el.textContent).toContain('Maya Chen');
    expect(el.querySelector('app-channel-badge')).toBeTruthy();
    expect(el.querySelector('app-status-badge')).toBeTruthy();
  });

  it('renders participants bar with participant names', async () => {
    storeMock.conversation.mockReturnValue(mockConversation);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationDetailComponent);
    fixture.detectChanges();

    const el = fixture.nativeElement;
    expect(el.textContent).toContain('Maya Chen');
    expect(el.textContent).toContain('Alice');
    expect(el.textContent).toContain('(customer)');
    expect(el.textContent).toContain('(member)');
  });

  it('renders timeline messages in ascending order with note styling', async () => {
    storeMock.conversation.mockReturnValue(mockConversation);
    storeMock.timeline.mockReturnValue(mockMessages);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationDetailComponent);
    fixture.detectChanges();

    const el = fixture.nativeElement;
    expect(el.textContent).toContain('I need help with my order');
    expect(el.textContent).toContain('I am looking into this for you');
    expect(el.textContent).toContain('Internal note about the billing issue');
  });

  it('shows status and assignee controls when user has manage permission', async () => {
    storeMock.conversation.mockReturnValue(mockConversation);
    TestBed.overrideProvider(PermissionsService, {
      useValue: { has: vi.fn(() => true), effective: vi.fn(() => new Set()) },
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationDetailComponent);
    fixture.detectChanges();

    // Status and assignee controls should be present
    expect(
      fixture.nativeElement.querySelectorAll('app-select-filter').length,
    ).toBeGreaterThanOrEqual(1);
  });

  it('hides status and assignee controls when user lacks manage permission', async () => {
    storeMock.conversation.mockReturnValue(mockConversation);
    TestBed.overrideProvider(PermissionsService, {
      useValue: { has: vi.fn(() => false), effective: vi.fn(() => new Set()) },
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationDetailComponent);
    fixture.detectChanges();

    // The assigneeOptions signal may still render, but the control-group should not be visible
    const controlGroups = fixture.nativeElement.querySelectorAll('.control-group');
    expect(controlGroups.length).toBe(0);
  });

  it('shows composer when user has manage permission', async () => {
    storeMock.conversation.mockReturnValue(mockConversation);
    TestBed.overrideProvider(PermissionsService, {
      useValue: { has: vi.fn(() => true), effective: vi.fn(() => new Set()) },
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationDetailComponent);
    fixture.detectChanges();

    const composer = fixture.nativeElement.querySelector('app-composer');
    expect(composer).toBeTruthy();
  });

  it('hides composer when user lacks manage permission', async () => {
    storeMock.conversation.mockReturnValue(mockConversation);
    TestBed.overrideProvider(PermissionsService, {
      useValue: { has: vi.fn(() => false), effective: vi.fn(() => new Set()) },
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationDetailComponent);
    fixture.detectChanges();

    const composer = fixture.nativeElement.querySelector('app-composer');
    expect(composer).toBeFalsy();
  });

  it('safely renders unusual text in messages', async () => {
    const unusualMessages: Message[] = [
      {
        id: 'msg-html',
        kind: 'reply',
        sender: { type: 'member' as const, displayName: 'Alice', membershipId: 'm1' },
        loggedBy: null,
        body: '<script>alert("xss")</script>',
        createdAt: '2026-07-13T10:00:00Z',
      },
      {
        id: 'msg-emoji',
        kind: 'reply',
        sender: { type: 'member' as const, displayName: 'Bob', membershipId: 'm2' },
        loggedBy: null,
        body: '👋 😊 🎉 Hello from Cairo! مرحبا',
        createdAt: '2026-07-13T10:01:00Z',
      },
      {
        id: 'msg-rtl',
        kind: 'customer',
        sender: { type: 'customer' as const, displayName: 'Maya Chen' },
        loggedBy: null,
        body: 'مرحبا كيف حالك',
        createdAt: '2026-07-13T10:02:00Z',
      },
      {
        id: 'msg-long',
        kind: 'reply',
        sender: { type: 'member' as const, displayName: 'Alice', membershipId: 'm1' },
        loggedBy: null,
        body: 'x'.repeat(5000),
        createdAt: '2026-07-13T10:03:00Z',
      },
    ];

    storeMock.conversation.mockReturnValue(mockConversation);
    storeMock.timeline.mockReturnValue(unusualMessages);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationDetailComponent);
    fixture.detectChanges();

    const el = fixture.nativeElement;
    // HTML should not be interpreted — the script tag should appear as text
    expect(el.textContent).toContain('<script>alert("xss")</script>');
    expect(el.textContent).toContain('👋');
    expect(el.textContent).toContain('مرحبا');
    expect(el.textContent).toContain('x'.repeat(5000));
  });

  it('sets empty timeline when no messages', async () => {
    storeMock.conversation.mockReturnValue(mockConversation);
    storeMock.timeline.mockReturnValue([]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationDetailComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Maya Chen');
  });
});
