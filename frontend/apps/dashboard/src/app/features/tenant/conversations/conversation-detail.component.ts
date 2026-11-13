import { ChangeDetectionStrategy, Component, inject, OnInit, computed } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';
import { map } from 'rxjs';
import { Permission } from '../../../core/authz/permissions';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { ConversationStatus } from '../../../core/api/tenant-api.models';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { ChannelBadgeComponent } from '../../../shared/components/channel-badge/channel-badge.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import {
  SelectFilterComponent,
  SelectFilterOption,
} from '../../../shared/components/select-filter/select-filter.component';
import { SatisfactionBadgeComponent } from '../../../shared/components/satisfaction-badge/satisfaction-badge.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { EscalationRoutingBannerComponent } from '../../../shared/components/escalation-routing-banner/escalation-routing-banner.component';
import { AiHandlingBannerComponent } from './ai-handling-banner.component';
import { ConversationsApiService } from './conversations-api.service';
import { ConversationDetailStore } from './conversation-detail.store';
import { ConversationSummaryComponent } from './conversation-summary.component';
import { ConversationThreadComponent } from './conversation-thread.component';
import { ComposerComponent } from './composer.component';
import { onTenantChange } from '../../../core/state/tenant-scope';
import { ApiError } from '../../../core/api/api.models';
import { ErrorKeyOverrides, errorMessageKey } from '../../../core/errors/error-message';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { LocaleDatePipe } from '../../../core/i18n/format';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

/** The wording of the errors this page can get beyond the shared ones (WhatsApp send failures). */
const ERROR_KEYS: ErrorKeyOverrides = {
  whatsapp_window_expired: marker('conversations.detail.errors.whatsappWindowExpired'),
  whatsapp_channel_disconnected: marker('conversations.detail.errors.whatsappDisconnected'),
  whatsapp_body_too_long: marker('conversations.detail.errors.whatsappTooLong'),
};

@Component({
  selector: 'app-conversation-detail',
  imports: [
    AvatarComponent,
    ChannelBadgeComponent,
    ComposerComponent,
    ConversationThreadComponent,
    EmptyStateComponent,
    ErrorReferenceComponent,
    LocaleDatePipe,
    AiHandlingBannerComponent,
    ConversationSummaryComponent,
    EscalationRoutingBannerComponent,
    LoadingStateComponent,
    SatisfactionBadgeComponent,
    SelectFilterComponent,
    StatusBadgeComponent,
    TranslocoPipe,
  ],
  providers: [provideTranslocoScope('conversations'), ConversationDetailStore],
  templateUrl: './conversation-detail.component.html',
  styles: [
    `
      .detail-shell {
        height: calc(100dvh - var(--app-topbar-height) - (var(--app-page-padding-y) * 2));
        display: grid;
        grid-template-rows: auto auto 1fr auto;
        min-height: 0;
        background: var(--app-panel);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-xl);
        overflow: hidden;
      }
      .detail-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-4);
        padding: var(--app-space-4);
        border-bottom: 1px solid var(--app-border);
        background: var(--app-panel);
      }
      .header-left {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        min-width: 0;
      }
      .back-btn {
        height: 34px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font-weight: 600;
        cursor: pointer;
      }
      .back-btn:hover {
        background: var(--app-panel-2);
      }
      .header-copy {
        min-width: 0;
        display: grid;
        gap: 4px;
      }
      .header-copy strong {
        color: var(--app-text);
        font-size: var(--app-font-base);
      }
      .header-channel {
        display: flex;
        gap: 6px;
        flex-wrap: wrap;
        align-items: center;
      }
      .widget-badge {
        display: inline-flex;
        align-items: center;
        padding: 1px 7px;
        border-radius: 999px;
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
        font-size: 10px;
        font-weight: 700;
        white-space: nowrap;
      }
      .header-right {
        display: flex;
        gap: var(--app-space-4);
        align-items: flex-start;
      }
      .control-group {
        display: grid;
        gap: 4px;
      }
      .control-label {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
      }
      .participants-bar {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        padding: var(--app-space-2) var(--app-space-4);
        border-bottom: 1px solid var(--app-border);
        background: var(--app-panel-2);
        overflow-x: auto;
      }
      .participants-label {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        margin-right: var(--app-space-1);
      }
      .participant-chip {
        display: inline-flex;
        align-items: center;
        gap: 4px;
        padding: 2px 8px;
        border-radius: 999px;
        background: var(--app-panel);
        border: 1px solid var(--app-border);
        color: var(--app-text);
        font-size: var(--app-font-xs);
        font-weight: 600;
        white-space: nowrap;
      }
      .participant-chip .inactive {
        color: var(--app-text-3);
        text-decoration: line-through;
      }
      .feedback-section {
        padding: var(--app-space-3) var(--app-space-4);
        border-bottom: 1px solid var(--app-border);
        background: var(--app-panel-2);
      }
      .feedback-header {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        margin-bottom: var(--app-space-1);
      }
      .feedback-label {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.05em;
      }
      .feedback-comment {
        margin: var(--app-space-1) 0;
        color: var(--app-text);
        font-size: var(--app-font-sm);
        line-height: 1.4;
      }
      .feedback-time {
        display: block;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .no-rating {
        margin: 0;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
        font-style: italic;
      }
      .participant-type {
        color: var(--app-text-3);
        font-weight: 400;
      }
      @media (max-width: 768px) {
        .detail-header {
          flex-direction: column;
          align-items: stretch;
        }
        .header-right {
          flex-wrap: wrap;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ConversationDetailComponent implements OnInit {
  protected readonly store = inject(ConversationDetailStore);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly permissions = inject(PermissionsService);
  private readonly api = inject(ConversationsApiService);

  private readonly locale = inject(LocaleService);

  protected readonly statusOptions = computed<SelectFilterOption[]>(() => [
    { value: 'open', label: this.locale.translate(marker('status.open')) },
    {
      value: 'pending',
      label: this.locale.translate(marker('status.pending')),
    },
    {
      value: 'resolved',
      label: this.locale.translate(marker('status.resolved')),
    },
    { value: 'closed', label: this.locale.translate(marker('status.closed')) },
  ]);
  protected readonly members = toSignal(this.api.listAssignableMembers().pipe(map((r) => r.data)), {
    initialValue: [],
  });

  private readonly assignable = toSignal(
    this.api.listAssignableMembers().pipe(map((members) => members.data)),
    { initialValue: [] },
  );

  protected readonly assigneeOptions = computed<SelectFilterOption[]>(() => [
    { value: '', label: this.locale.translate(marker('common.unassigned')) },
    ...this.assignable().map((m) => ({ value: m.id, label: m.displayName })),
  ]);

  protected readonly hasManagePerm = () =>
    this.permissions.has('conversations.manage' as Permission);

  constructor() {
    // The conversation belongs to the workspace that was active; the new one has no such id.
    onTenantChange(() => this.goBack());
  }

  ngOnInit(): void {
    const id = this.route.snapshot.paramMap.get('id');
    if (id) this.store.openConversation(id);
  }

  protected goBack(): void {
    this.router.navigate(['..'], { relativeTo: this.route });
  }

  protected customerInitials(conv: { customer: { displayName: string } }): string {
    return conv.customer.displayName
      .split(' ')
      .map((p) => p[0])
      .join('')
      .toUpperCase()
      .slice(0, 2);
  }

  protected channelBadge(channel: string) {
    return channel as 'email' | 'phone' | 'web_chat' | 'whatsapp' | 'telegram';
  }

  protected statusTone(status: string): 'green' | 'amber' | 'red' | 'neutral' {
    if (status === 'closed' || status === 'resolved') return 'green';
    if (status === 'pending') return 'red';
    if (status === 'open') return 'amber';
    return 'neutral';
  }

  protected assigneeValue(conv: { assignee: { membershipId: string } | null }): string {
    return conv.assignee?.membershipId ?? '';
  }

  protected onStatusChange(id: string, status: string): void {
    this.store.patchStatus(id, status as ConversationStatus);
  }

  protected onAssigneeChange(id: string, membershipId: string): void {
    this.store.patchAssignment(id, membershipId || null);
  }

  /** Catalog key of the text for an error of this page. */
  protected errorKey(error: ApiError): string {
    return errorMessageKey(error, ERROR_KEYS);
  }
}
