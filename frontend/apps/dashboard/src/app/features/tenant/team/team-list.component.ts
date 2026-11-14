import { LocaleDatePipe } from '../../../core/i18n/format';
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
import { MfaService } from '../../../core/auth/mfa.service';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { Store } from '@ngrx/store';
import { selectActiveTenant } from '../../../core/state/tenant-context.feature';
import { onTenantChange } from '../../../core/state/tenant-scope';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { AvailabilityDotComponent } from '../../../shared/components/availability-dot/availability-dot.component';
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
import { DialogShellComponent } from '../../../shared/components/dialog-shell/dialog-shell.component';
import { InviteDialogComponent } from './invite-dialog.component';
import { InvitationTableComponent } from './invitation-table.component';
import { RoleSelectComponent } from './role-select.component';
import { SkillsManagerComponent } from './skills-manager.component';
import { TeamStore } from './team.store';
import { ApiError } from '../../../core/api/api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { invitationStatusKey, roleLabelKey } from './team-labels';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

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
    LocaleDatePipe,
    AvailabilityDotComponent,
    ButtonComponent,
    DataTableComponent,
    DialogShellComponent,
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
    SkillsManagerComponent,
    StatusBadgeComponent,
    ToolbarComponent,
    TranslocoPipe,
    ErrorReferenceComponent,
  ],
  providers: [provideTranslocoScope('team'), TeamStore],
  templateUrl: './team-list.component.html',
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
      .member-info {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
      }
      .skill-chips {
        display: flex;
        gap: 4px;
        flex-wrap: wrap;
        margin-left: auto;
      }
      .skill-chip {
        display: inline-block;
        padding: 1px 6px;
        border-radius: 4px;
        background: var(--app-panel-2);
        color: var(--app-text-2);
        font-size: 11px;
        font-weight: 600;
        white-space: nowrap;
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
  private readonly mfa = inject(MfaService);
  private readonly permissions = inject(PermissionsService);
  private readonly currentUserService = inject(CurrentUserService);
  private readonly globalStore = inject(Store, { optional: true });

  private readonly locale = inject(LocaleService);

  protected readonly showInviteDialog = signal(false);
  protected readonly showSkillsManager = signal(false);
  protected readonly resetTarget = signal<TeamMember | null>(null);
  protected readonly resetting = signal(false);
  protected readonly resetResult = signal<{
    tone: 'success' | 'error';
    key: string;
    params?: Record<string, unknown>;
    requestId?: string;
  } | null>(null);
  protected readonly invitationSubmitting = signal(false);
  protected readonly invitationError = signal<ApiError | null>(null);
  protected readonly invitationResult = signal<CreateInvitationResponse | null>(null);
  private readonly activeTenant =
    this.globalStore?.selectSignal(selectActiveTenant) ?? (() => null);

  protected readonly memberStatusOptions = computed<SelectFilterOption[]>(() => [
    { value: 'all', label: this.locale.translate(marker('team.page.allStatus')) },
    { value: 'active', label: this.locale.translate(marker('status.active')) },
    { value: 'disabled', label: this.locale.translate(marker('status.disabled')) },
  ]);
  protected readonly invitationStatusOptions = computed<SelectFilterOption[]>(() => [
    { value: 'all', label: this.locale.translate(marker('team.page.allInvitations')) },
    { value: 'pending', label: this.locale.translate(marker('status.pending')) },
    { value: 'expired', label: this.locale.translate(marker('status.expired')) },
    { value: 'accepted', label: this.locale.translate(marker('status.accepted')) },
    { value: 'revoked', label: this.locale.translate(marker('status.revoked')) },
  ]);

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

  /** The accessible name of a member's role control. */
  protected memberRoleLabel(member: TeamMember): string {
    return this.locale.translate(marker('team.members.changeRole'), { name: member.displayName });
  }

  protected roleLabelKey(role: string): string {
    return roleLabelKey(role);
  }

  protected statusLabelKey(status: string): string {
    return invitationStatusKey(status);
  }

  /** Catalog key of the text for an error of this page; an absent error gets the generic one. */
  protected errorKey(error: ApiError | null): string {
    return errorMessageKey(error ?? { code: 'unknown_error', message: '', status: 0 });
  }

  protected onStatusChange(value: string): void {
    this.store.filterByStatus(value === 'all' ? undefined : (value as MemberStatus));
  }

  protected onInvitationStatusChange(value: string): void {
    this.store.setInvitationStatusFilter(value === 'all' ? undefined : (value as InvitationStatus));
  }

  /** Catalog key of the heading of a filtered list of invitations. */
  protected invitationSectionTitle(status: InvitationStatus | 'all'): string {
    switch (status) {
      case 'pending':
        return marker('team.invitations.pending');
      case 'accepted':
        return marker('team.invitations.accepted');
      case 'revoked':
        return marker('team.invitations.revoked');
      case 'expired':
        return marker('team.invitations.expired');
      default:
        return marker('team.invitations.all');
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

  /** A workspace owner, or a platform administrator looking at a workspace, can reset a member. */
  protected canResetMfa(member: TeamMember): boolean {
    return (
      !this.isSelf(member) && (this.canAssignOwner() || this.permissions.has('platform.admin'))
    );
  }

  protected askResetMfa(member: TeamMember): void {
    this.resetResult.set(null);
    this.resetTarget.set(member);
  }

  protected cancelResetMfa(): void {
    if (this.resetting()) return;
    this.resetTarget.set(null);
  }

  protected async confirmResetMfa(): Promise<void> {
    const target = this.resetTarget();
    if (!target) return;
    this.resetting.set(true);
    try {
      // The server checks who may reset whom; an owner only for a member who belongs to this
      // workspace alone, a platform administrator for anyone.
      if (this.canAssignOwner()) {
        await this.mfa.resetMember(target.id);
      } else {
        await this.mfa.resetUser(target.userId);
      }
      this.resetResult.set({
        tone: 'success',
        key: marker('team.reset.done'),
        params: { name: target.displayName },
      });
    } catch (error) {
      const status = (error as { status?: number } | null)?.status;
      const code = (error as { code?: string } | null)?.code;
      const requestId = (error as { requestId?: string } | null)?.requestId;
      this.resetResult.set({
        tone: 'error',
        requestId,
        key:
          code === 'mfa_step_up_required'
            ? marker('team.reset.cancelled')
            : status === 403
              ? marker('team.reset.forbidden')
              : marker('team.reset.failed'),
      });
    } finally {
      this.resetting.set(false);
      this.resetTarget.set(null);
    }
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
    // What this page holds itself belongs to the old workspace too: an open dialog, a picked
    // member, a result message.
    onTenantChange(() => {
      this.closeInviteDialog();
      this.resetTarget.set(null);
      this.resetResult.set(null);
      this.invitationResult.set(null);
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
