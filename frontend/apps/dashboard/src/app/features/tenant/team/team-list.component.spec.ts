import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Store } from '@ngrx/store';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { TeamMember } from '../../../core/api/tenant-api.models';
import { APP_CONFIG } from '../../../core/config/app-config';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { TeamApiService } from './team-api.service';
import { TeamListComponent } from './team-list.component';

describe('TeamListComponent', () => {
  const mockApi = {
    getMembers: vi.fn(),
    getInvitations: vi.fn(),
    createInvitation: vi.fn(),
    revokeInvitation: vi.fn(),
    patchMember: vi.fn(),
  };

  const mockPermissions = { has: vi.fn().mockReturnValue(true) };

  const mockCurrentUserService = {
    currentUser: vi.fn().mockReturnValue({
      id: 'current-user',
      memberships: [{ role: 'owner' }],
    }),
  };

  // Actor is 'owner' rank 5 by default, so both admin (rank 4) and agent (rank 2)
  // members below are manageable and their row actions should render.
  const members: TeamMember[] = [
    {
      id: 'm-1',
      userId: 'u-1',
      displayName: 'Alice',
      email: 'alice@test.com',
      role: 'admin',
      status: 'active',
      joinedAt: '2026-01-15T00:00:00Z',
    },
    {
      id: 'm-2',
      userId: 'u-2',
      displayName: 'Bob',
      email: 'bob@test.com',
      role: 'agent',
      status: 'disabled',
      joinedAt: '2026-02-20T00:00:00Z',
    },
  ];

  function configureModule(extraProviders: unknown[] = []) {
    return TestBed.configureTestingModule({
      imports: [TeamListComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: APP_CONFIG, useValue: { publicDashboardUrl: 'https://dashboard.example.com' } },
        { provide: TeamApiService, useValue: mockApi },
        { provide: PermissionsService, useValue: mockPermissions },
        { provide: CurrentUserService, useValue: mockCurrentUserService },
        ...extraProviders,
      ],
    });
  }

  beforeEach(() => {
    mockApi.getMembers.mockReset();
    mockApi.getInvitations.mockReset();
    mockApi.createInvitation.mockReset();
    mockApi.revokeInvitation.mockReset();
    mockApi.patchMember.mockReset();
    mockPermissions.has.mockReturnValue(true);
    mockCurrentUserService.currentUser.mockReturnValue({
      id: 'current-user',
      memberships: [{ role: 'owner' }],
    });
  });

  it('renders the team page header', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Team');
    });
  });

  it('shows empty state when no members', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
    expect(fixture.nativeElement.textContent).toContain('No team members');
  });

  it('keeps search and invite controls visible when the roster is empty (FR-001/FR-014)', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockPermissions.has.mockReturnValue(true);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-search-input')).toBeTruthy();
      expect(fixture.nativeElement.textContent).toContain('Invite');
    });
  });

  it('renders member rows in the data table', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Alice');
      expect(fixture.nativeElement.textContent).toContain('Bob');
    });
  });

  it('shows invite button when user can manage', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockPermissions.has.mockReturnValue(true);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Invite');
    });
  });

  it('hides invite button when user cannot manage', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockPermissions.has.mockReturnValue(false);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).not.toContain('Invite');
    });
  });

  it('shows pending and expired invitations in separate sections', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({
        data: {
          items: [
            {
              id: 'i-1',
              email: 'new@test.com',
              role: 'agent',
              status: 'pending',
              invitedByName: 'Admin',
              emailDeliveryStatus: 'unconfigured',
              createdAt: '2026-01-01T00:00:00Z',
              expiresAt: '2026-02-01T00:00:00Z',
            },
            {
              id: 'i-2',
              email: 'stale@test.com',
              role: 'agent',
              status: 'expired',
              invitedByName: 'Admin',
              emailDeliveryStatus: 'unconfigured',
              createdAt: '2025-01-01T00:00:00Z',
              expiresAt: '2025-02-01T00:00:00Z',
            },
          ],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    mockPermissions.has.mockReturnValue(true);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Pending invitations');
      expect(fixture.nativeElement.textContent).toContain('new@test.com');
      expect(fixture.nativeElement.textContent).toContain('Expired invitations');
      expect(fixture.nativeElement.textContent).toContain('stale@test.com');
    });
  });

  it('renders invitation groups as accessible shared data tables', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({
        data: {
          items: [
            {
              id: 'i-1',
              email: 'new@test.com',
              role: 'agent',
              status: 'pending',
              invitedByName: 'Admin',
              emailDeliveryStatus: 'unconfigured',
              createdAt: '2026-01-01T00:00:00Z',
              expiresAt: '2026-02-01T00:00:00Z',
            },
          ],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('new@test.com');
    });

    const invitationTable = fixture.nativeElement.querySelector(
      'section[aria-labelledby="pending-invitations-title"] app-data-table table',
    );
    expect(invitationTable).toBeTruthy();
    expect(invitationTable.querySelector('caption')?.textContent).toContain('Pending invitations');
    expect(invitationTable.querySelectorAll('th[scope="col"]')).toHaveLength(6);
    expect(invitationTable.querySelector('button')?.getAttribute('aria-label')).toBe(
      'Revoke invitation for new@test.com',
    );
    expect(fixture.nativeElement.querySelectorAll('app-invitation-table')).toHaveLength(1);
  });

  it('gives the member roster a caption, scoped headers, and contextual row controls', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Alice');
    });

    const roster = fixture.nativeElement.querySelector('app-data-table table');
    expect(roster.querySelector('caption')?.textContent).toContain('Team members');
    expect(roster.querySelectorAll('th[scope="col"]')).toHaveLength(6);
    expect(
      fixture.nativeElement
        .querySelector('app-role-select app-choice-group [role="group"]')
        ?.getAttribute('aria-label'),
    ).toBe("Change Alice's role");
    const statusButtons = Array.from(
      fixture.nativeElement.querySelectorAll('app-button button') as NodeListOf<HTMLButtonElement>,
    );
    expect(
      statusButtons.find((button) => button.textContent?.trim() === 'Disable')?.ariaLabel,
    ).toBe('Disable Alice');
    expect(statusButtons.find((button) => button.textContent?.trim() === 'Enable')?.ariaLabel).toBe(
      'Enable Bob',
    );
  });

  it('renders invitation load errors instead of no invitations', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(throwError(() => new Error('Invitation load failed')));
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Unable to load invitations');
      expect(fixture.nativeElement.textContent).toContain('Invitation load failed');
      expect(fixture.nativeElement.textContent).not.toContain('No invitations found');
    });
  });

  it('switches the invitation list to accepted invitations from the status filter', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValueOnce(
      of({
        data: {
          items: [
            {
              id: 'i-1',
              email: 'new@test.com',
              role: 'agent',
              status: 'pending',
              invitedByName: 'Admin',
              emailDeliveryStatus: 'unconfigured',
              createdAt: '2026-01-01T00:00:00Z',
              expiresAt: '2026-02-01T00:00:00Z',
            },
            {
              id: 'i-2',
              email: 'joined@test.com',
              role: 'agent',
              status: 'accepted',
              invitedByName: 'Admin',
              emailDeliveryStatus: 'unconfigured',
              createdAt: '2026-01-02T00:00:00Z',
              expiresAt: '2026-02-02T00:00:00Z',
            },
          ],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    mockApi.getInvitations.mockReturnValueOnce(
      of({
        data: {
          items: [
            {
              id: 'i-2',
              email: 'joined@test.com',
              role: 'agent',
              status: 'accepted',
              invitedByName: 'Admin',
              emailDeliveryStatus: 'unconfigured',
              createdAt: '2026-01-02T00:00:00Z',
              expiresAt: '2026-02-02T00:00:00Z',
            },
          ],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    mockPermissions.has.mockReturnValue(true);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const filters = fixture.nativeElement.querySelectorAll('app-select-filter');
      expect(filters.length).toBeGreaterThanOrEqual(2);
    });

    const invitationFilter = fixture.nativeElement.querySelectorAll(
      'app-select-filter select',
    )[1] as HTMLSelectElement;
    invitationFilter.value = 'accepted';
    invitationFilter.dispatchEvent(new Event('change', { bubbles: true }));
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(mockApi.getInvitations).toHaveBeenCalledWith({ limit: 25, status: 'accepted' });
      expect(fixture.nativeElement.textContent).toContain('joined@test.com');
    });
  });

  it('hides the revoke button for an invitation whose role outranks the actor', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({
        data: {
          items: [
            {
              id: 'i-1',
              email: 'senior@test.com',
              role: 'admin',
              status: 'pending',
              invitedByName: 'Admin',
              emailDeliveryStatus: 'unconfigured',
              createdAt: '2026-01-01T00:00:00Z',
              expiresAt: '2026-02-01T00:00:00Z',
            },
          ],
          nextCursor: null,
          hasMore: false,
        },
      }),
    );
    mockCurrentUserService.currentUser.mockReturnValue({
      id: 'current-user',
      memberships: [
        {
          tenantId: 'tenant-1',
          tenantName: 'Acme',
          tenantSlug: 'acme',
          role: 'manager',
        },
      ],
    });
    mockPermissions.has.mockImplementation((perm: string) => perm !== 'owner.assign');
    const activeTenant = vi.fn(() => ({
      id: 'tenant-1',
      name: 'Acme',
      slug: 'acme',
      status: 'active',
      plan: 'trial',
    }));
    configureModule([{ provide: Store, useValue: { selectSignal: vi.fn(() => activeTenant) } }]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('senior@test.com');
    });
    expect(fixture.nativeElement.querySelector('.revoke-btn')).toBeNull();
  });

  it('uses the active tenant membership when resolving the current role', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockCurrentUserService.currentUser.mockReturnValue({
      id: 'current-user',
      memberships: [
        { tenantId: 'tenant-1', tenantName: 'One', tenantSlug: 'one', role: 'viewer' },
        { tenantId: 'tenant-2', tenantName: 'Two', tenantSlug: 'two', role: 'owner' },
      ],
    });
    mockPermissions.has.mockImplementation(
      (perm: string) => perm === 'members.manage' || perm === 'owner.assign',
    );
    const activeTenant = vi.fn(() => ({
      id: 'tenant-2',
      name: 'Two',
      slug: 'two',
      status: 'active',
      plan: 'trial',
    }));
    configureModule([{ provide: Store, useValue: { selectSignal: vi.fn(() => activeTenant) } }]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const ownerOptionVisible = Array.from(
        fixture.nativeElement.querySelectorAll(
          'app-role-select app-choice-group button',
        ) as NodeListOf<HTMLButtonElement>,
      ).some((button) => button.textContent?.trim() === 'Owner');
      expect(ownerOptionVisible).toBe(true);
    });
  });

  it('shows dialog when invite button is clicked', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockPermissions.has.mockReturnValue(true);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const inviteBtn = Array.from(
        fixture.nativeElement.querySelectorAll(
          'app-button button',
        ) as NodeListOf<HTMLButtonElement>,
      ).find((button) => button.textContent?.trim() === 'Invite');
      if (inviteBtn) {
        inviteBtn.click();
        fixture.detectChanges();
      }
      expect(fixture.componentInstance['showInviteDialog']()).toBe(true);
    });
  });

  it('shows role select and disable buttons when the actor outranks the members', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockPermissions.has.mockReturnValue(true);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const roleSelects = fixture.nativeElement.querySelectorAll('app-role-select');
      expect(roleSelects.length).toBe(2);
      const actionBtns = Array.from(
        fixture.nativeElement.querySelectorAll(
          'app-button button',
        ) as NodeListOf<HTMLButtonElement>,
      ).filter((button) => ['Disable', 'Enable'].includes(button.textContent?.trim() ?? ''));
      expect(actionBtns.length).toBe(2);
    });
  });

  it('hides role select and disable buttons when user cannot manage members', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockPermissions.has.mockReturnValue(false);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-role-select')).toBeNull();
      const actionBtns = Array.from(
        fixture.nativeElement.querySelectorAll(
          'app-button button',
        ) as NodeListOf<HTMLButtonElement>,
      ).filter((button) => ['Disable', 'Enable'].includes(button.textContent?.trim() ?? ''));
      expect(actionBtns.length).toBe(0);
    });
  });

  it('hides row actions for a member whose rank is at or above the actor (rank-gated, FR-016)', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    // A manager (rank 3) may not act on Alice, an admin (rank 4), but may act on Bob, an
    // agent (rank 2).
    mockCurrentUserService.currentUser.mockReturnValue({
      id: 'current-user',
      memberships: [
        {
          tenantId: 'tenant-1',
          tenantName: 'Acme',
          tenantSlug: 'acme',
          role: 'manager',
        },
      ],
    });
    mockPermissions.has.mockReturnValue(true);
    const activeTenant = vi.fn(() => ({
      id: 'tenant-1',
      name: 'Acme',
      slug: 'acme',
      status: 'active',
      plan: 'trial',
    }));
    configureModule([{ provide: Store, useValue: { selectSignal: vi.fn(() => activeTenant) } }]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const roleSelects = fixture.nativeElement.querySelectorAll('app-role-select');
      expect(roleSelects.length).toBe(1);
      const actionBtns = Array.from(
        fixture.nativeElement.querySelectorAll(
          'app-button button',
        ) as NodeListOf<HTMLButtonElement>,
      ).filter((button) => ['Disable', 'Enable'].includes(button.textContent?.trim() ?? ''));
      expect(actionBtns.length).toBe(1);
    });
  });

  it('shows "Disable" for active members and "Enable" for disabled members', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockPermissions.has.mockReturnValue(true);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const buttons = Array.from(
        fixture.nativeElement.querySelectorAll(
          'app-button button',
        ) as NodeListOf<HTMLButtonElement>,
      ).filter((button) => ['Disable', 'Enable'].includes(button.textContent?.trim() ?? ''));
      expect(buttons.length).toBe(2);
      expect(buttons[0].textContent).toContain('Disable');
      expect(buttons[1].textContent).toContain('Enable');
    });
  });

  it('calls patchMember with disabled status when clicking Disable, and reloads on success', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.patchMember.mockReturnValue(of({ data: members[0] }));
    mockPermissions.has.mockReturnValue(true);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const buttons = Array.from(
        fixture.nativeElement.querySelectorAll(
          'app-button button',
        ) as NodeListOf<HTMLButtonElement>,
      ).filter((button) => ['Disable', 'Enable'].includes(button.textContent?.trim() ?? ''));
      expect(buttons.length).toBe(2);
      buttons[0].click();
      fixture.detectChanges();
    });
    expect(mockApi.patchMember).toHaveBeenCalledWith('m-1', { status: 'disabled' });
    // getMembers is called once on init and once again after the mutation succeeds.
    await vi.waitFor(() => {
      expect(mockApi.getMembers).toHaveBeenCalledTimes(2);
    });
  });

  it('surfaces a mutation error near the row instead of only logging to console', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.patchMember.mockReturnValue(
      throwError(() => ({ message: 'Cannot disable the last owner' })),
    );
    mockPermissions.has.mockReturnValue(true);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const buttons = Array.from(
        fixture.nativeElement.querySelectorAll(
          'app-button button',
        ) as NodeListOf<HTMLButtonElement>,
      ).filter((button) => ['Disable', 'Enable'].includes(button.textContent?.trim() ?? ''));
      buttons[0].click();
      fixture.detectChanges();
    });
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Cannot disable the last owner');
      expect(fixture.nativeElement.querySelector('app-inline-alert [role="alert"]')).toBeTruthy();
    });
  });

  it('calls patchMember with the new role when a row role-select changes', async () => {
    mockApi.getMembers.mockReturnValue(
      of({ data: { items: members, nextCursor: null, hasMore: false } }),
    );
    mockApi.getInvitations.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false } }),
    );
    mockApi.patchMember.mockReturnValue(of({ data: members[0] }));
    mockPermissions.has.mockReturnValue(true);
    configureModule();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TeamListComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelectorAll('app-role-select').length).toBe(2);
    });

    // Drive the change through the component method directly (role-select internals are
    // covered by role-select.component.spec.ts and must not be re-tested here).
    fixture.componentInstance['changeRole'](members[0], 'manager');
    expect(mockApi.patchMember).toHaveBeenCalledWith('m-1', { role: 'manager' });
  });
});
