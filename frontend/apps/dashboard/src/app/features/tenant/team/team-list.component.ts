import { DatePipe } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import {
  CreateInvitationPayload,
  CreateInvitationResponse,
  InvitationStatus,
  MemberStatus,
  MembershipRole,
  TeamMember,
  TenantInvitation,
} from '../../../core/api/tenant-api.models';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { Store } from '@ngrx/store';
import { selectActiveTenant } from '../../../core/state/tenant-context.feature';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { SearchInputComponent } from '../../../shared/components/search-input/search-input.component';
import {
  SelectFilterComponent,
  SelectFilterOption,
} from '../../../shared/components/select-filter/select-filter.component';
import {
  BadgeTone,
  StatusBadgeComponent,
} from '../../../shared/components/status-badge/status-badge.component';
import { MEMBER_STATUS_TONES, MEMBERSHIP_ROLE_TONES } from '../../../core/ui/status-badge-config';
import { ToolbarComponent } from '../../../shared/components/toolbar/toolbar.component';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { InviteDialogComponent } from './invite-dialog.component';
import { InvitationTableComponent } from './invitation-table.component';
import { RoleSelectComponent } from './role-select.component';
import { TeamStore } from './team.store';

const ROLE_RANKS: Record<MembershipRole, number> = {
  owner: 5,
  admin: 4,
  manager: 3,
  agent: 2,
  viewer: 1,
};

@Component({
  selector: 'app-team-list',
  imports: [
    DatePipe,
    ButtonComponent,
    DataTableComponent,
    EmptyStateComponent,
    AvatarComponent,
    InviteDialogComponent,
    InvitationTableComponent,
    InlineAlertComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    RoleSelectComponent,
    SearchInputComponent,
    SelectFilterComponent,
    StatusBadgeComponent,
    ToolbarComponent,
  ],
  providers: [TeamStore],
  template: `
    <app-page-container>
      <app-page-header title="Team" description="Manage team members and invitations" />

      @switch (store.status()) {
        @case ('loading') {
          @if (store.members().length === 0) {
            <app-loading-state label="Loading team roster..." />
          }
        }
        @case ('error') {
          <app-empty-state
            icon="@tui.alert-circle"
            title="Something went wrong"
            description="We couldn't load the team roster. Please try again."
          >
            <app-button (pressed)="store.retry()">Try again</app-button>
          </app-empty-state>
        }
      }

      @if (
        store.status() !== 'error' &&
        !(store.status() === 'loading' && store.members().length === 0)
      ) {
        <div class="stack">
          <app-toolbar>
            <app-search-input
              toolbar-start
              placeholder="Search members"
              [value]="store.query().q ?? ''"
              (valueChange)="store.search($event)"
            />
            <app-select-filter
              toolbar-end
              label="Status filter"
              [value]="store.query().status ?? 'all'"
              [options]="memberStatusOptions"
              (valueChange)="onStatusChange($event)"
            />
            <app-select-filter
              toolbar-end
              label="Invitation status filter"
              [value]="invitationFilter()"
              [options]="invitationStatusOptions"
              (valueChange)="onInvitationStatusChange($event)"
            />
            @if (canManage()) {
              <app-button toolbar-end variant="primary" (pressed)="openInviteDialog($event)">
                Invite
              </app-button>
            }
          </app-toolbar>

          @if (store.invitationsStatus() === 'error') {
            <app-empty-state
              icon="@tui.alert-circle"
              title="Unable to load invitations"
              [description]="
                store.invitationsError() ?? 'We could not load invitations. Please try again.'
              "
            >
              <app-button (pressed)="store.loadInvitations({ query: store.invitationQuery() })">
                Try again
              </app-button>
            </app-empty-state>
          } @else {
            @if (invitationFilter() === 'all') {
              @if (pendingInvitations().length > 0) {
                <app-invitation-table
                  title="Pending invitations"
                  headingId="pending-invitations-title"
                  [invitations]="pendingInvitations()"
                  [revocableIds]="revocableInvitationIds()"
                  [revokingId]="revokingInvitationId()"
                  [showActions]="true"
                  (revoke)="revoke($event)"
                >
                  @if (store.revokeInvitationStatus() === 'error') {
                    <app-inline-alert tone="error">{{
                      store.revokeInvitationError()
                    }}</app-inline-alert>
                  }
                </app-invitation-table>
              }

              @if (expiredInvitations().length > 0) {
                <app-invitation-table
                  title="Expired invitations"
                  headingId="expired-invitations-title"
                  [invitations]="expiredInvitations()"
                />
              }

              @if (otherInvitations().length > 0) {
                <app-invitation-table
                  title="Accepted and revoked invitations"
                  headingId="invitation-history-title"
                  [invitations]="otherInvitations()"
                />
              }
            } @else {
              @if (filteredInvitations().length > 0) {
                <app-invitation-table
                  [title]="invitationSectionTitle(invitationFilter())"
                  headingId="filtered-invitations-title"
                  [invitations]="filteredInvitations()"
                  [revocableIds]="revocableInvitationIds()"
                  [revokingId]="revokingInvitationId()"
                  [showActions]="true"
                  (revoke)="revoke($event)"
                />
              } @else {
                <app-empty-state
                  icon="@tui.users"
                  title="No invitations found"
                  description="Try a different invitation status filter."
                />
              }
            }
          }

          @if (store.invitationHasMore()) {
            <div class="load-more invitations-load-more">
              <app-button (pressed)="store.loadMoreInvitations()">Load more invitations</app-button>
            </div>
          }

          @if (store.status() === 'empty') {
            <app-empty-state
              icon="@tui.users"
              title="No team members"
              description="Invite team members to collaborate on support conversations."
            />
          } @else {
            <app-data-table>
              <table>
                <caption>
                  Team members
                </caption>
                <thead>
                  <tr>
                    <th scope="col">Name</th>
                    <th scope="col">Email</th>
                    <th scope="col">Role</th>
                    <th scope="col">Status</th>
                    <th scope="col">Joined</th>
                    @if (canManage()) {
                      <th scope="col">Actions</th>
                    }
                  </tr>
                </thead>
                <tbody>
                  @for (member of store.members(); track member.id) {
                    <tr>
                      <td class="cell-name">
                        <app-avatar [initials]="memberInitials(member.displayName)" size="sm" />
                        <strong>{{ member.displayName }}</strong>
                      </td>
                      <td class="muted">{{ member.email }}</td>
                      <td>
                        <app-status-badge [status]="member.role" [tone]="roleTone(member.role)" />
                      </td>
                      <td>
                        <app-status-badge
                          [status]="member.status"
                          [tone]="statusTone(member.status)"
                        />
                      </td>
                      <td class="muted">{{ member.joinedAt | date: 'mediumDate' }}</td>
                      @if (canManage()) {
                        <td class="actions-cell">
                          @if (!isSelf(member) && canManageMember(member)) {
                            <app-role-select
                              [value]="member.role"
                              [currentRole]="currentRole()"
                              [canAssignOwner]="canAssignOwner()"
                              [ariaLabel]="memberRoleLabel(member)"
                              (valueChange)="changeRole(member, $event)"
                            />
                            <app-button
                              variant="ghost"
                              [ariaLabel]="
                                (member.status === 'active' ? 'Disable ' : 'Enable ') +
                                member.displayName
                              "
                              [disabled]="isMemberUpdating(member.id)"
                              (pressed)="toggleStatus(member)"
                            >
                              {{
                                isMemberUpdating(member.id)
                                  ? 'Working…'
                                  : member.status === 'active'
                                    ? 'Disable'
                                    : 'Enable'
                              }}
                            </app-button>
                            @if (
                              store.memberUpdateStatus() === 'error' &&
                              store.memberUpdateMemberId() === member.id
                            ) {
                              <app-inline-alert tone="error">{{
                                store.memberUpdateError()
                              }}</app-inline-alert>
                            }
                          }
                        </td>
                      }
                    </tr>
                  }
                </tbody>
              </table>
            </app-data-table>

            @if (store.hasMore()) {
              <div class="load-more">
                <app-button (pressed)="store.loadNext()" [disabled]="store.status() === 'loading'">
                  @if (store.status() === 'loading') {
                    Loading...
                  } @else {
                    Load more
                  }
                </app-button>
              </div>
            }
          }
        </div>
      }
    </app-page-container>

    @if (showInviteDialog()) {
      <app-invite-dialog
        (invite)="onInvite($event)"
        (closeDialog)="closeInviteDialog()"
        [submitting]="invitationSubmitting()"
        [error]="invitationError()"
        [result]="invitationResult()"
        [deliveryPollingError]="store.invitationDeliveryPollingError()"
        [currentRole]="currentRole()"
        [canAssignOwner]="canAssignOwner()"
      />
    }
  `,
  styles: [
    `
      .stack {
        display: grid;
        gap: var(--app-space-4);
      }
      .cell-name {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
      }
      strong,
      small {
        display: block;
      }
      strong {
        color: var(--app-text);
      }
      .load-more {
        display: flex;
        justify-content: center;
        padding: var(--app-space-3);
      }
      app-button[toolbar-end] {
        margin-left: auto;
      }
      caption {
        position: absolute;
        width: 1px;
        height: 1px;
        overflow: hidden;
        clip: rect(0 0 0 0);
      }
      .actions-cell {
        display: flex;
        gap: var(--app-space-2);
        align-items: center;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TeamListComponent {
  protected readonly store = inject(TeamStore);
  private readonly permissions = inject(PermissionsService);
  private readonly currentUserService = inject(CurrentUserService);
  private readonly globalStore = inject(Store, { optional: true });

  protected readonly showInviteDialog = signal(false);
  protected readonly invitationSubmitting = signal(false);
  protected readonly invitationError = signal<string | null>(null);
  protected readonly invitationResult = signal<CreateInvitationResponse | null>(null);
  private readonly activeTenant =
    this.globalStore?.selectSignal(selectActiveTenant) ?? (() => null);

  protected readonly memberStatusOptions: SelectFilterOption[] = [
    { value: 'all', label: 'All status' },
    { value: 'active', label: 'Active' },
    { value: 'disabled', label: 'Disabled' },
  ];
  protected readonly invitationStatusOptions: SelectFilterOption[] = [
    { value: 'all', label: 'All invitations' },
    { value: 'pending', label: 'Pending' },
    { value: 'expired', label: 'Expired' },
    { value: 'accepted', label: 'Accepted' },
    { value: 'revoked', label: 'Revoked' },
  ];

  protected readonly canManage = computed(() => this.permissions.has('members.manage'));
  protected readonly currentUserId = computed(() => this.currentUserService.currentUser()?.id);
  protected readonly currentRole = computed(() => {
    const activeTenantId = this.activeTenant()?.id;
    const membership =
      activeTenantId == null
        ? undefined
        : this.currentUserService
            .currentUser()
            ?.memberships?.find((item) => item.tenantId === activeTenantId);

    return (
      membership?.role ??
      (this.permissions.has('owner.assign')
        ? 'owner'
        : this.permissions.has('members.manage')
          ? 'admin'
          : 'viewer')
    );
  });
  protected readonly canAssignOwner = computed(() => this.permissions.has('owner.assign'));
  protected readonly invitationFilter = computed(
    () => this.store.invitationQuery().status ?? 'all',
  );
  protected readonly pendingInvitations = computed(() =>
    this.store.invitations().filter((invitation) => invitation.status === 'pending'),
  );
  protected readonly expiredInvitations = computed(() =>
    this.store.invitations().filter((invitation) => invitation.status === 'expired'),
  );
  protected readonly otherInvitations = computed(() =>
    this.store
      .invitations()
      .filter((invitation) => invitation.status !== 'pending' && invitation.status !== 'expired'),
  );
  protected readonly filteredInvitations = computed(() => {
    const status = this.store.invitationQuery().status;
    if (!status) return this.store.invitations();

    return this.store.invitations().filter((invitation) => invitation.status === status);
  });
  protected readonly revocableInvitationIds = computed(() =>
    this.store
      .invitations()
      .filter((invitation) => this.canRevokeInvitation(invitation))
      .map((invitation) => invitation.id),
  );
  protected readonly revokingInvitationId = computed(() =>
    this.store.revokeInvitationStatus() === 'submitting' ? this.store.revokeInvitationId() : null,
  );

  protected readonly roleToneMap: Record<MembershipRole, BadgeTone> = {
    ...MEMBERSHIP_ROLE_TONES,
  };

  protected readonly statusToneMap: Record<MemberStatus, BadgeTone> = {
    ...MEMBER_STATUS_TONES,
  };

  protected roleTone(role: MembershipRole): BadgeTone {
    return this.roleToneMap[role];
  }

  protected statusTone(status: MemberStatus): BadgeTone {
    return this.statusToneMap[status];
  }

  protected memberInitials(displayName: string): string {
    return displayName
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((part) => part[0] ?? '')
      .join('')
      .toUpperCase();
  }

  protected memberRoleLabel(member: TeamMember): string {
    return `Change ${member.displayName}'s role`;
  }

  protected onStatusChange(value: string): void {
    this.store.filterByStatus(value === 'all' ? undefined : (value as MemberStatus));
  }

  protected onInvitationStatusChange(value: string): void {
    this.store.setInvitationStatusFilter(value === 'all' ? undefined : (value as InvitationStatus));
  }

  protected invitationSectionTitle(status: InvitationStatus | 'all'): string {
    switch (status) {
      case 'pending':
        return 'Pending invitations';
      case 'accepted':
        return 'Accepted invitations';
      case 'revoked':
        return 'Revoked invitations';
      case 'expired':
        return 'Expired invitations';
      default:
        return 'Invitations';
    }
  }

  protected onInvite(payload: CreateInvitationPayload): void {
    this.invitationSubmitting.set(true);
    this.invitationError.set(null);
    this.invitationResult.set(null);
    this.store.createInvitation(payload);
  }

  protected openInviteDialog(event: MouseEvent): void {
    this.inviteTrigger = event.currentTarget as HTMLElement | null;
    this.showInviteDialog.set(true);
  }

  protected revoke(id: string): void {
    this.store.revokeInvitation(id);
  }

  protected closeInviteDialog(): void {
    this.showInviteDialog.set(false);
    this.invitationResult.set(null);
    this.invitationError.set(null);
    this.store.clearLastCreatedInvitation();
    queueMicrotask(() => {
      this.inviteTrigger?.focus();
      this.inviteTrigger = null;
    });
  }

  protected isSelf(member: TeamMember): boolean {
    return member.userId === this.currentUserId();
  }

  /**
   * Mirrors the backend's rank rule (contracts/permissions.md, rule 1): an actor may act
   * on a target only if their rank is strictly above the target's, except an Owner may
   * also act on another Owner. Presentation only — the server re-checks on every request.
   */
  protected canManageMember(member: TeamMember): boolean {
    const actorRank = ROLE_RANKS[this.currentRole()] ?? 0;
    const targetRank = ROLE_RANKS[member.role] ?? 0;
    if (this.currentRole() === 'owner' && member.role === 'owner') return true;
    return actorRank > targetRank;
  }

  /**
   * Revoking an invitation follows the same assign-at-or-below rank rule as creating one
   * (contracts/permissions.md rule 5): an actor may only revoke invitations for roles they
   * could themselves assign.
   */
  protected canRevokeInvitation(invitation: TenantInvitation): boolean {
    if (invitation.status !== 'pending') return false;
    if (!this.canManage()) return false;
    const actorRank = ROLE_RANKS[this.currentRole()] ?? 0;
    const invitationRank = ROLE_RANKS[invitation.role] ?? 0;
    if (invitation.role === 'owner') return this.canAssignOwner();
    return invitationRank <= actorRank;
  }

  protected isMemberUpdating(memberId: string): boolean {
    return (
      this.store.memberUpdateStatus() === 'submitting' &&
      this.store.memberUpdateMemberId() === memberId
    );
  }

  protected changeRole(member: TeamMember, role: MembershipRole): void {
    this.store.changeMemberRole(member.id, role);
  }

  protected toggleStatus(member: TeamMember): void {
    const newStatus: MemberStatus = member.status === 'active' ? 'disabled' : 'active';
    this.store.setMemberStatus(member.id, newStatus);
  }

  constructor() {
    let initialized = false;
    let previousTenantId: string | null = null;
    effect(() => {
      const tenantId = this.activeTenant()?.id ?? null;
      if (!initialized) {
        initialized = true;
        previousTenantId = tenantId;
        return;
      }

      if (tenantId === previousTenantId) return;
      previousTenantId = tenantId;
      this.closeInviteDialog();
    });

    effect(() => {
      const createStatus = this.store.createInvitationStatus();
      this.invitationSubmitting.set(createStatus === 'submitting');
      if (createStatus === 'error') {
        this.invitationError.set(this.store.createInvitationError());
      }

      const last = this.store.lastCreatedInvitation();
      if (last && this.showInviteDialog()) {
        this.invitationSubmitting.set(false);
        this.invitationResult.set(last);
      }
    });

    effect(() => {
      void this.store.invitationsStatus();
      if (this.showInviteDialog()) {
        return;
      }
      this.invitationSubmitting.set(false);
    });
  }

  private inviteTrigger: HTMLElement | null = null;
}
