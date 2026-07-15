import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { APP_CONFIG } from '../../../core/config/app-config';
import { QueueEntry } from '../../../core/api/tenant-api.models';
import { EscalationQueueStore } from './escalation-queue.store';
import { EscalationQueueComponent } from './escalation-queue.component';

describe('EscalationQueueComponent', () => {
  let storeMock: {
    items: ReturnType<typeof vi.fn>;
    loading: ReturnType<typeof vi.fn>;
    error: ReturnType<typeof vi.fn>;
    claim: ReturnType<typeof vi.fn>;
    loadQueue: ReturnType<typeof vi.fn>;
  };
  let permissionsMock: { has: ReturnType<typeof vi.fn> };

  const mockEntry: QueueEntry = {
    escalation: {
      id: 'e-1',
      conversationId: 'c-1',
      reason: 'customer_requested',
      requiredSkills: [{ id: null, name: 'billing' }],
      status: 'queued',
      routing: null,
      escalatedAt: '2026-07-14T10:00:00Z',
      closedAt: null,
    },
    conversation: { id: 'c-1', channel: 'web_chat', customer: { id: 'cu-1', name: 'Maya Chen' } },
    waitingSeconds: 500,
  };

  const mockEntry2: QueueEntry = {
    escalation: {
      id: 'e-2',
      conversationId: 'c-2',
      reason: 'skill_escalated',
      requiredSkills: [],
      status: 'queued',
      routing: null,
      escalatedAt: '2026-07-14T11:00:00Z',
      closedAt: null,
    },
    conversation: { id: 'c-2', channel: 'email', customer: { id: 'cu-2', name: 'Jon Bell' } },
    waitingSeconds: 30,
  };

  function createStoreMock() {
    return {
      items: vi.fn(() => []),
      loading: vi.fn(() => false),
      error: vi.fn(() => null),
      claim: vi.fn(),
      loadQueue: vi.fn(),
    };
  }

  beforeEach(() => {
    storeMock = createStoreMock();
    permissionsMock = { has: vi.fn(() => true) };
    TestBed.configureTestingModule({
      imports: [EscalationQueueComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: EscalationQueueStore, useValue: storeMock },
        { provide: PermissionsService, useValue: permissionsMock },
        { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1', production: false } },
      ],
    });
  });

  it('renders empty state when no items', async () => {
    storeMock.items.mockReturnValue([]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(EscalationQueueComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('No escalations in queue');
  });

  it('renders rows oldest-first by waiting time', async () => {
    storeMock.items.mockReturnValue([mockEntry, mockEntry2]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(EscalationQueueComponent);
    fixture.detectChanges();

    const rows = fixture.nativeElement.querySelectorAll('tbody tr');
    expect(rows.length).toBe(2);
    expect(rows[0].textContent).toContain('billing');
    expect(rows[1].textContent).toContain('skill_escalated');
  });

  it('calls store.claim on claim button click in at most 2 interactions', async () => {
    storeMock.items.mockReturnValue([mockEntry]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(EscalationQueueComponent);
    fixture.detectChanges();

    const claimBtn = fixture.nativeElement.querySelector('.claim-btn') as HTMLButtonElement;
    expect(claimBtn).toBeTruthy();

    claimBtn.click();
    expect(storeMock.claim).toHaveBeenCalledWith('e-1');
  });

  it('hides claim button when user lacks conversations.manage', async () => {
    permissionsMock.has.mockReturnValue(false);
    storeMock.items.mockReturnValue([mockEntry]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(EscalationQueueComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('.claim-btn')).toBeFalsy();
  });

  it('shows 409 message and removes row when claim conflicts', async () => {
    storeMock.error.mockReturnValue('This escalation was already claimed by another agent.');
    storeMock.items.mockReturnValue([]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(EscalationQueueComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('already claimed');
  });
});
