import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { Store } from '@ngrx/store';
import { defer, of, Subject, throwError } from 'rxjs';
import {
  CreateInvitationResponse,
  TeamMember,
  TenantInvitation,
  TenantSummary,
} from '../../../core/api/tenant-api.models';
import { TeamApiService } from './team-api.service';
import { TeamStore } from './team.store';

describe('TeamStore', () => {
  const mockApi = {
    getMembers: vi.fn(),
    getInvitations: vi.fn(),
    getInvitationDelivery: vi.fn(),
    createInvitation: vi.fn(),
    revokeInvitation: vi.fn(),
    patchMember: vi.fn(),
  };

  const mockInvitation: TenantInvitation = {
    id: 'i-1',
    email: 'user@test.com',
    role: 'agent',
    status: 'pending',
    invitedByName: 'Admin',
    emailDeliveryStatus: 'unconfigured',
    createdAt: '2026-01-01T00:00:00Z',
    expiresAt: '2026-02-01T00:00:00Z',
  };

  const mockMember: TeamMember = {
    id: 'm-1',
    userId: 'u-1',
    displayName: 'Alice',
    email: 'alice@t.com',
    role: 'admin',
    status: 'active',
    joinedAt: '2026-01-01T00:00:00Z',
  };

  beforeEach(() => {
    mockApi.getMembers.mockReset();
    mockApi.getInvitations.mockReset();
    mockApi.getInvitationDelivery.mockReset();
    mockApi.createInvitation.mockReset();
    mockApi.revokeInvitation.mockReset();
    mockApi.patchMember.mockReset();
  });

  function configureStore() {
    TestBed.configureTestingModule({
      providers: [TeamStore, { provide: TeamApiService, useValue: mockApi }],
    });
    return TestBed.inject(TeamStore);
  }

  function configureStoreWithTenant(activeTenant = signal<TenantSummary | null>(null)) {
    TestBed.configureTestingModule({
      providers: [
        TeamStore,
        { provide: TeamApiService, useValue: mockApi },
        { provide: Store, useValue: { selectSignal: () => activeTenant } },
      ],
    });
    return { store: TestBed.inject(TeamStore), activeTenant };
  }

  it('initializes state', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    expect(store.members()).toEqual([]);
    expect(store.query()).toEqual({ limit: 25 });
    expect(store.nextCursor()).toBeNull();
    expect(store.hasMore()).toBe(false);
    expect(store.error()).toBeNull();
    expect(store.invitations()).toEqual([]);
    expect(store.lastCreatedInvitation()).toBeNull();
  });

  it('loads invitations on init', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [mockInvitation], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    expect(store.invitations()).toEqual([mockInvitation]);
    expect(store.invitationsStatus()).toBe('success');
  });

  it('loads more invitations with cursor pagination', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValueOnce(
      of({ data: { items: [mockInvitation], nextCursor: 'cursor-1', hasMore: true } }),
    );
    mockApi.getInvitations.mockReturnValueOnce(
      of({
        data: {
          items: [
            {
              ...mockInvitation,
              id: 'i-2',
              email: 'second@test.com',
            },
          ],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    store.loadMoreInvitations();

    expect(mockApi.getInvitations).toHaveBeenCalledWith({ limit: 25 });
    expect(mockApi.getInvitations).toHaveBeenCalledWith({ limit: 25, cursor: 'cursor-1' });
    expect(store.invitations().map((item) => item.email)).toEqual([
      'user@test.com',
      'second@test.com',
    ]);
    expect(store.invitationHasMore()).toBe(false);
  });

  it('resets invitation pagination when the status filter changes', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValueOnce(
      of({ data: { items: [mockInvitation], nextCursor: 'cursor-1', hasMore: true } }),
    );
    mockApi.getInvitations.mockReturnValueOnce(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    store.setInvitationStatusFilter('accepted');

    expect(store.invitationQuery()).toEqual({ limit: 25, status: 'accepted' });
    expect(store.invitations()).toEqual([]);
    expect(mockApi.getInvitations).toHaveBeenCalledWith({ limit: 25 });
    expect(mockApi.getInvitations).toHaveBeenCalledWith({ limit: 25, status: 'accepted' });
  });

  it('transitions invitations to error when load fails', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(throwError(() => new Error('Network error')));
    const store = configureStore();
    TestBed.flushEffects();
    expect(store.invitationsStatus()).toBe('error');
    expect(store.invitationsError()).toBe('Network error');
  });

  it('creates invitation and captures result', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    const createResponse: CreateInvitationResponse = {
      invitation: mockInvitation,
      acceptUrl: 'https://example.com/invite/token',
      emailSent: true,
      emailDeliveryStatus: 'sent',
    };
    mockApi.createInvitation.mockReturnValue(of({ data: createResponse }));

    store.createInvitation({ email: 'user@test.com', role: 'agent' });

    expect(mockApi.createInvitation).toHaveBeenCalledWith({
      email: 'user@test.com',
      role: 'agent',
    });
  });

  it('polls a queued invitation until sent and updates list and dialog result', async () => {
    vi.useFakeTimers();
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    const queued = { ...mockInvitation, emailDeliveryStatus: 'queued' as const };
    const sent = { ...queued, emailDeliveryStatus: 'sent' as const };
    mockApi.createInvitation.mockReturnValue(
      of({
        data: {
          invitation: queued,
          acceptUrl: 'https://example.com/invite/token',
          emailSent: false,
          emailDeliveryStatus: 'queued' as const,
        },
      }),
    );
    mockApi.getInvitationDelivery
      .mockReturnValueOnce(of({ data: { emailDeliveryStatus: queued.emailDeliveryStatus } }))
      .mockReturnValueOnce(of({ data: { emailDeliveryStatus: sent.emailDeliveryStatus } }));

    store.createInvitation({ email: queued.email, role: queued.role });
    await vi.advanceTimersByTimeAsync(0);
    expect(store.lastCreatedInvitation()?.emailDeliveryStatus).toBe('queued');
    await vi.advanceTimersByTimeAsync(1000);

    expect(store.lastCreatedInvitation()?.emailDeliveryStatus).toBe('sent');
    expect(store.lastCreatedInvitation()?.emailSent).toBe(true);
    expect(store.invitations()[0].emailDeliveryStatus).toBe('sent');
    await vi.advanceTimersByTimeAsync(3000);
    expect(mockApi.getInvitationDelivery).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });

  it('surfaces a polling operation error after three consecutive transient failures', async () => {
    vi.useFakeTimers();
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    const queued = { ...mockInvitation, emailDeliveryStatus: 'queued' as const };
    mockApi.createInvitation.mockReturnValue(
      of({
        data: {
          invitation: queued,
          acceptUrl: 'https://example.com/invite/token',
          emailSent: false,
          emailDeliveryStatus: 'queued' as const,
        },
      }),
    );
    let requestAttempts = 0;
    mockApi.getInvitationDelivery.mockReturnValue(
      defer(() => {
        requestAttempts += 1;
        return throwError(() => new Error('temporary'));
      }),
    );

    store.createInvitation({ email: queued.email, role: queued.role });
    await vi.advanceTimersByTimeAsync(3000);

    expect(requestAttempts).toBe(3);
    expect(store.invitationDeliveryPollingError()).toContain('delivery status');
    vi.useRealTimers();
  });

  it('cancels targeted polling when the invitation result is reset', async () => {
    vi.useFakeTimers();
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    const queued = { ...mockInvitation, emailDeliveryStatus: 'queued' as const };
    mockApi.createInvitation.mockReturnValue(
      of({
        data: {
          invitation: queued,
          acceptUrl: '/invite/token',
          emailSent: false,
          emailDeliveryStatus: 'queued' as const,
        },
      }),
    );
    const statusResponse = new Subject<{ data: { emailDeliveryStatus: 'queued' } }>();
    mockApi.getInvitationDelivery.mockReturnValue(statusResponse);

    store.createInvitation({ email: queued.email, role: queued.role });
    await vi.advanceTimersByTimeAsync(0);
    expect(statusResponse.observed).toBe(true);
    store.clearLastCreatedInvitation();
    expect(statusResponse.observed).toBe(false);
    vi.useRealTimers();
  });

  it('resets the transient failure sequence after a successful queued poll', async () => {
    vi.useFakeTimers();
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    const queued = { ...mockInvitation, emailDeliveryStatus: 'queued' as const };
    mockApi.createInvitation.mockReturnValue(
      of({
        data: {
          invitation: queued,
          acceptUrl: '/invite/token',
          emailSent: false,
          emailDeliveryStatus: 'queued' as const,
        },
      }),
    );
    let poll = 0;
    let firstPollSubscriptions = 0;
    mockApi.getInvitationDelivery.mockImplementation(() => {
      poll += 1;
      if (poll === 1) {
        return defer(() => {
          firstPollSubscriptions += 1;
          return firstPollSubscriptions < 3
            ? throwError(() => new Error('temporary'))
            : of({ data: { emailDeliveryStatus: 'queued' as const } });
        });
      }
      return throwError(() => new Error('temporary again'));
    });

    store.createInvitation({ email: queued.email, role: queued.role });
    await vi.advanceTimersByTimeAsync(5000);

    expect(firstPollSubscriptions).toBe(3);
    expect(poll).toBe(2);
    expect(store.invitationDeliveryPollingError()).toContain('delivery status');
    vi.useRealTimers();
  });

  it('revokes invitation and reloads', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [mockInvitation], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    mockApi.revokeInvitation.mockReturnValue(of({ data: undefined }));
    mockApi.getInvitations.mockReset();
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );

    store.revokeInvitation('i-1');

    expect(mockApi.revokeInvitation).toHaveBeenCalledWith('i-1');
  });

  it('clears lastCreatedInvitation', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    store.clearLastCreatedInvitation();
    expect(store.lastCreatedInvitation()).toBeNull();
  });

  it('transitions to empty when the initial load returns no members', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    expect(store.status()).toBe('empty');
    expect(store.members()).toEqual([]);
  });

  it('transitions to success when members are loaded', () => {
    const members: TeamMember[] = [
      {
        id: 'm-1',
        userId: 'u-1',
        displayName: 'Alice',
        email: 'a@t.com',
        role: 'admin',
        status: 'active',
        joinedAt: '2026-01-01T00:00:00Z',
      },
    ];
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: 'cursor-2', hasMore: true } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    expect(store.status()).toBe('success');
    expect(store.members()).toEqual(members);
    expect(store.nextCursor()).toBe('cursor-2');
    expect(store.hasMore()).toBe(true);
  });

  it('transitions to error when load fails', () => {
    mockApi.getMembers.mockReturnValue(throwError(() => new Error('Network error')));
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    expect(store.status()).toBe('error');
    expect(store.error()).toBe('Network error');
  });

  it('resets and reloads when the active tenant changes', () => {
    const tenant1: TenantSummary = {
      id: 'tenant-1',
      name: 'Tenant One',
      slug: 'tenant-one',
      status: 'active',
      plan: 'trial',
    };
    const tenant2: TenantSummary = {
      id: 'tenant-2',
      name: 'Tenant Two',
      slug: 'tenant-two',
      status: 'active',
      plan: 'trial',
    };
    const activeTenant = signal<TenantSummary | null>(tenant1);

    mockApi.getMembers.mockReturnValueOnce(
      of({
        data: {
          items: [
            {
              id: 'm-1',
              userId: 'u-1',
              displayName: 'Alice',
              email: 'alice@tenant-one.test',
              role: 'admin',
              status: 'active',
              joinedAt: '2026-01-01T00:00:00Z',
            },
          ],
          nextCursor: 'cursor-1',
          hasMore: true,
        },
      }),
    );
    mockApi.getMembers.mockReturnValueOnce(
      of({
        data: {
          items: [
            {
              id: 'm-2',
              userId: 'u-2',
              displayName: 'Bob',
              email: 'bob@tenant-two.test',
              role: 'manager',
              status: 'disabled',
              joinedAt: '2026-02-01T00:00:00Z',
            },
          ],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    mockApi.getInvitations.mockReturnValueOnce(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValueOnce(
      of({
        data: {
          items: [
            {
              id: 'i-1',
              email: 'new@tenant-two.test',
              role: 'agent',
              status: 'pending',
              invitedByName: 'Admin',
              emailDeliveryStatus: 'unconfigured',
              createdAt: '2026-02-15T00:00:00Z',
              expiresAt: '2026-02-22T00:00:00Z',
            },
          ],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );

    const { store } = configureStoreWithTenant(activeTenant);
    TestBed.flushEffects();

    store.setQuery({ q: 'alice', cursor: 'cursor-1' });
    expect(store.query()).toEqual({ limit: 25, q: 'alice', cursor: 'cursor-1' });

    activeTenant.set(tenant2);
    TestBed.flushEffects();

    expect(store.members()[0]?.displayName).toBe('Bob');
    expect(store.invitations()[0]?.email).toBe('new@tenant-two.test');
    expect(store.query()).toEqual({ limit: 25 });
    expect(mockApi.getMembers).toHaveBeenCalledTimes(2);
    expect(mockApi.getInvitations).toHaveBeenCalledTimes(2);
  });

  it('retry calls loadMembers with current query', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    mockApi.getMembers.mockReset();
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );

    store.retry();
    expect(mockApi.getMembers).toHaveBeenCalledWith({ limit: 25 });
  });

  it('loadNext appends the next page onto the existing members (pagination accumulate)', () => {
    mockApi.getMembers.mockReturnValueOnce(
      of({
        data: {
          items: [{ ...mockMember, id: 'm-1' }],
          nextCursor: 'cursor-1',
          hasMore: true,
        },
      }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    expect(store.members().length).toBe(1);

    mockApi.getMembers.mockReturnValueOnce(
      of({
        data: {
          items: [{ ...mockMember, id: 'm-2' }],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    store.loadNext();
    expect(store.members().map((m) => m.id)).toEqual(['m-1', 'm-2']);
    expect(store.hasMore()).toBe(false);
    expect(mockApi.getMembers).toHaveBeenCalledWith({ limit: 25, cursor: 'cursor-1' });
  });

  it('a fresh search (not append) resets the member list rather than accumulating', () => {
    vi.useFakeTimers();
    try {
      mockApi.getMembers.mockReturnValueOnce(
        of({ data: { items: [{ ...mockMember, id: 'm-1' }], nextCursor: null, hasMore: false } }),
      );
      mockApi.getInvitations.mockReturnValue(
        of({ data: { items: [], nextCursor: null, hasMore: false } }),
      );
      const store = configureStore();
      TestBed.flushEffects();
      expect(store.members().length).toBe(1);

      mockApi.getMembers.mockReturnValueOnce(
        of({ data: { items: [{ ...mockMember, id: 'm-2' }], nextCursor: null, hasMore: false } }),
      );
      store.search('alice');
      vi.advanceTimersByTime(300);
      expect(store.members().map((m) => m.id)).toEqual(['m-2']);
    } finally {
      vi.useRealTimers();
    }
  });

  it('debounces and cancels superseded search requests', () => {
    vi.useFakeTimers();
    try {
      mockApi.getMembers.mockReturnValue(
        of({ data: { items: [], nextCursor: null, hasMore: false } }),
      );
      mockApi.getInvitations.mockReturnValue(
        of({ data: { items: [], nextCursor: null, hasMore: false } }),
      );
      const store = configureStore();
      TestBed.flushEffects();
      mockApi.getMembers.mockReset();
      mockApi.getMembers.mockReturnValue(
        of({ data: { items: [], nextCursor: null, hasMore: false } }),
      );

      store.search('a');
      vi.advanceTimersByTime(100);
      store.search('al');
      vi.advanceTimersByTime(100);
      store.search('ali');
      vi.advanceTimersByTime(300);

      expect(mockApi.getMembers).toHaveBeenCalledTimes(1);
      expect(mockApi.getMembers).toHaveBeenCalledWith(expect.objectContaining({ q: 'ali' }));
    } finally {
      vi.useRealTimers();
    }
  });

  it('surfaces a revoke-invitation error instead of only logging to console', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [mockInvitation], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    mockApi.revokeInvitation.mockReturnValue(
      throwError(() => ({ message: 'Invitation already accepted', status: 409 })),
    );
    store.revokeInvitation('i-1');

    expect(store.revokeInvitationStatus()).toBe('error');
    expect(store.revokeInvitationError()).toBe('Invitation already accepted');
    expect(store.revokeInvitationId()).toBe('i-1');
    // The stale invitation list is left as-is; loadInvitations is not re-triggered on error.
    expect(store.invitations()).toEqual([mockInvitation]);
  });

  it('clears revoke-invitation status/error and reloads the list on success', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValueOnce(
      of({ data: { items: [mockInvitation], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    mockApi.revokeInvitation.mockReturnValue(of({ data: undefined }));
    mockApi.getInvitations.mockReturnValueOnce(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    store.revokeInvitation('i-1');

    expect(store.revokeInvitationStatus()).toBe('success');
    expect(store.revokeInvitationError()).toBeNull();
    expect(store.revokeInvitationId()).toBeNull();
    expect(store.invitations()).toEqual([]);
  });

  it('clearRevokeInvitationError resets revoke state', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [mockInvitation], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    mockApi.revokeInvitation.mockReturnValue(throwError(() => ({ message: 'boom' })));
    store.revokeInvitation('i-1');
    expect(store.revokeInvitationStatus()).toBe('error');

    store.clearRevokeInvitationError();
    expect(store.revokeInvitationStatus()).toBe('idle');
    expect(store.revokeInvitationError()).toBeNull();
    expect(store.revokeInvitationId()).toBeNull();
  });

  it('changeMemberRole calls patchMember with the role payload and reloads members on success', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [mockMember], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    mockApi.patchMember.mockReturnValue(of({ data: { ...mockMember, role: 'manager' } }));
    store.changeMemberRole('m-1', 'manager');

    expect(mockApi.patchMember).toHaveBeenCalledWith('m-1', { role: 'manager' });
    expect(store.memberUpdateStatus()).toBe('success');
    expect(store.memberUpdateMemberId()).toBeNull();
    expect(mockApi.getMembers).toHaveBeenCalledTimes(2);
  });

  it('setMemberStatus surfaces a per-member error on failure (e.g. last-owner 409)', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [mockMember], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    mockApi.patchMember.mockReturnValue(
      throwError(() => ({ message: 'Cannot disable the last owner', status: 409 })),
    );
    store.setMemberStatus('m-1', 'disabled');

    expect(mockApi.patchMember).toHaveBeenCalledWith('m-1', { status: 'disabled' });
    expect(store.memberUpdateStatus()).toBe('error');
    expect(store.memberUpdateError()).toBe('Cannot disable the last owner');
    expect(store.memberUpdateMemberId()).toBe('m-1');
  });

  it('clearMemberUpdateError resets member-mutation state', () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [mockMember], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    mockApi.patchMember.mockReturnValue(throwError(() => ({ message: 'boom' })));
    store.setMemberStatus('m-1', 'disabled');
    expect(store.memberUpdateStatus()).toBe('error');

    store.clearMemberUpdateError();
    expect(store.memberUpdateStatus()).toBe('idle');
    expect(store.memberUpdateError()).toBeNull();
    expect(store.memberUpdateMemberId()).toBeNull();
  });

  it('concurrent updates to different members do not cancel one another (mergeMap, not switchMap)', () => {
    const memberTwo: TeamMember = { ...mockMember, id: 'm-2', displayName: 'Bob' };
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [mockMember, memberTwo], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    const firstSubject = new Subject<{ data: TeamMember }>();
    const secondSubject = new Subject<{ data: TeamMember }>();
    mockApi.patchMember.mockReturnValueOnce(firstSubject);
    mockApi.patchMember.mockReturnValueOnce(secondSubject);

    store.changeMemberRole('m-1', 'manager');
    expect(store.memberUpdateMemberId()).toBe('m-1');

    // Start a second mutation for a different member before the first resolves — with
    // mergeMap (not switchMap) the first in-flight request must not be torn down.
    store.setMemberStatus('m-2', 'disabled');
    expect(store.memberUpdateMemberId()).toBe('m-2');

    // Resolving the first request late must still land its success state, proving it
    // was not cancelled when the second mutation started.
    firstSubject.next({ data: { ...mockMember, role: 'manager' } });
    firstSubject.complete();
    expect(mockApi.patchMember).toHaveBeenCalledWith('m-1', { role: 'manager' });
    expect(mockApi.patchMember).toHaveBeenCalledWith('m-2', { status: 'disabled' });

    secondSubject.next({ data: { ...memberTwo, status: 'disabled' } });
    secondSubject.complete();
  });
});
