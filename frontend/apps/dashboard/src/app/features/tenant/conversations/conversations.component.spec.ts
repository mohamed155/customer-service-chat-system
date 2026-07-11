import { provideZonelessChangeDetection, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { RouterModule } from '@angular/router';
import { Store } from '@ngrx/store';
import { provideTaiga } from '@taiga-ui/core';
import { Subject, of, throwError } from 'rxjs';
import { CONVERSATION_FIXTURES } from '../../../shared/fixtures/conversation.fixtures';
import { CUSTOMER_FIXTURES } from '../../../shared/fixtures/customer.fixtures';
import { ConversationFixture, CustomerFixture } from '../../../shared/fixtures/fixture.models';
import { RoutedPageDataService } from '../routed-page-data.service';
import { ConversationsComponent } from './conversations.component';

type ConversationsPagePayload = {
  page: 'conversations';
  data: {
    conversations: readonly ConversationFixture[];
    customers: readonly CustomerFixture[];
  };
};

const conversationsPayload = (
  conversations: readonly ConversationFixture[],
  customers: readonly CustomerFixture[],
): ConversationsPagePayload => ({
  page: 'conversations',
  data: { conversations, customers },
});

describe('ConversationsComponent', () => {
  const loadConversations = vi.fn();
  const activeTenant = signal<{ id: string } | null>({ id: 'tenant-1' });

  beforeEach(() => {
    activeTenant.set({ id: 'tenant-1' });
    loadConversations.mockReset();
    TestBed.configureTestingModule({
      imports: [ConversationsComponent, RouterModule.forRoot([])],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: RoutedPageDataService, useValue: { load: loadConversations } },
        { provide: Store, useValue: { selectSignal: () => activeTenant } },
      ],
    });
  });

  it('moves from pending to content', async () => {
    loadConversations.mockReturnValue(
      of(conversationsPayload(CONVERSATION_FIXTURES, CUSTOMER_FIXTURES)),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });
  });

  it('updates the thread and customer panel when selecting a conversation', async () => {
    loadConversations.mockReturnValue(
      of(conversationsPayload(CONVERSATION_FIXTURES, CUSTOMER_FIXTURES)),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    const rows = Array.from(
      fixture.nativeElement.querySelectorAll(
        'app-inbox-list .item',
      ) as NodeListOf<HTMLButtonElement>,
    );
    const jonRow = rows.find((b) => b.textContent?.includes('Jon Bell'));
    jonRow?.click();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('jon.bell@example.com');
  });

  it('filters the rendered list by status', async () => {
    loadConversations.mockReturnValue(
      of(conversationsPayload(CONVERSATION_FIXTURES, CUSTOMER_FIXTURES)),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    const filters = Array.from(
      fixture.nativeElement.querySelectorAll(
        'app-inbox-list .filters button',
      ) as NodeListOf<HTMLButtonElement>,
    );
    const openFilter = filters.find((b) => b.textContent?.trim() === 'Open');
    openFilter?.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
      expect(fixture.nativeElement.textContent).not.toContain('Ava Patel');
    });
  });

  it('does not add an extra header landmark inside the dashboard page content', async () => {
    loadConversations.mockReturnValue(
      of(conversationsPayload(CONVERSATION_FIXTURES, CUSTOMER_FIXTURES)),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });
    expect(fixture.nativeElement.querySelectorAll('header').length).toBe(0);
  });

  it('moves from pending to the shared zero-data state', async () => {
    loadConversations.mockReturnValue(of(conversationsPayload([], [])));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
  });

  it('shows the shared no-results state and resets the status filter', async () => {
    loadConversations.mockReturnValue(
      of(conversationsPayload(CONVERSATION_FIXTURES.slice(0, 2), CUSTOMER_FIXTURES)),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    const filters = Array.from(
      fixture.nativeElement.querySelectorAll(
        'app-inbox-list .filters button',
      ) as NodeListOf<HTMLButtonElement>,
    );
    const closedFilter = filters.find((b) => b.textContent?.trim() === 'Closed');
    closedFilter?.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('No conversations match');
    });

    const showAllBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.includes('Show all conversations'));
    showAllBtn?.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
      expect(fixture.nativeElement.textContent).not.toContain('No conversations match');
    });
  });

  it('transitions from tenant A content to tenant B content', async () => {
    loadConversations.mockReturnValue(
      of(conversationsPayload(CONVERSATION_FIXTURES, CUSTOMER_FIXTURES)),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    loadConversations.mockReturnValue(
      of(conversationsPayload(CONVERSATION_FIXTURES.slice(2, 4), CUSTOMER_FIXTURES.slice(2, 4))),
    );
    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
      expect(fixture.nativeElement.textContent).not.toContain('Maya Chen');
    });
  });

  it('moves from pending to error and retries', async () => {
    loadConversations.mockReturnValue(throwError(() => new Error('load failed')));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    });

    loadConversations.mockReturnValue(
      of(conversationsPayload(CONVERSATION_FIXTURES, CUSTOMER_FIXTURES)),
    );
    const retryBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Try again')!;
    retryBtn.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });
  });

  it('prevents stale tenant A data from reappearing after tenant B resolves first', async () => {
    const aConversations = CONVERSATION_FIXTURES.slice(0, 2);
    const bConversations = CONVERSATION_FIXTURES.slice(2, 4);
    const bCustomers = CUSTOMER_FIXTURES.slice(2, 4);
    const subjects: Subject<ConversationsPagePayload>[] = [];

    loadConversations.mockImplementation(() => {
      const s = new Subject<ConversationsPagePayload>();
      subjects.push(s);
      return s.asObservable();
    });

    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => expect(loadConversations).toHaveBeenCalledTimes(1));

    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    await vi.waitFor(() => expect(loadConversations).toHaveBeenCalledTimes(2));

    subjects[1].next(conversationsPayload(bConversations, bCustomers));
    subjects[1].complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
    });

    subjects[0].next(conversationsPayload(aConversations, CUSTOMER_FIXTURES));
    subjects[0].complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
      expect(fixture.nativeElement.textContent).not.toContain('Maya Chen');
    });
  });

  it('removes tenant A content while tenant B is pending', async () => {
    const bConversations = CONVERSATION_FIXTURES.slice(2, 4);
    const bCustomers = CUSTOMER_FIXTURES.slice(2, 4);
    const subjects: Subject<ConversationsPagePayload>[] = [];

    loadConversations.mockReturnValue(
      of(conversationsPayload(CONVERSATION_FIXTURES.slice(0, 2), CUSTOMER_FIXTURES)),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    loadConversations.mockImplementation(() => {
      const s = new Subject<ConversationsPagePayload>();
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

    subjects[0].next(conversationsPayload(bConversations, bCustomers));
    subjects[0].complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
    });
  });

  it('handles tenant B resolving to empty', async () => {
    loadConversations.mockReturnValue(
      of(conversationsPayload(CONVERSATION_FIXTURES, CUSTOMER_FIXTURES)),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    loadConversations.mockReturnValue(of(conversationsPayload([], [])));
    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
      expect(fixture.nativeElement.textContent).not.toContain('Maya Chen');
    });
  });

  it('ignores rejected tenant A after tenant B loads successfully', async () => {
    const bConversations = CONVERSATION_FIXTURES.slice(2, 4);
    const bCustomers = CUSTOMER_FIXTURES.slice(2, 4);
    const subjects: Subject<ConversationsPagePayload>[] = [];

    loadConversations.mockImplementation(() => {
      const s = new Subject<ConversationsPagePayload>();
      subjects.push(s);
      return s.asObservable();
    });

    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();
    await vi.waitFor(() => expect(loadConversations).toHaveBeenCalledTimes(1));

    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();
    await vi.waitFor(() => expect(loadConversations).toHaveBeenCalledTimes(2));

    subjects[1].next(conversationsPayload(bConversations, bCustomers));
    subjects[1].complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
    });

    subjects[0].error(new Error('A failed'));
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
      expect(fixture.nativeElement.textContent).not.toContain('Maya Chen');
    });
  });
});
