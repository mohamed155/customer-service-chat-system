import { ChangeDetectionStrategy, Component, inject, OnInit } from '@angular/core';
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
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { EscalationBannerComponent } from '../escalations/escalation-banner.component';
import { AiHandlingBannerComponent } from './ai-handling-banner.component';
import { ConversationsApiService } from './conversations-api.service';
import { ConversationDetailStore } from './conversation-detail.store';
import { ConversationSummaryComponent } from './conversation-summary.component';
import { ConversationThreadComponent } from './conversation-thread.component';
import { ComposerComponent } from './composer.component';

const STATUS_OPTIONS: SelectFilterOption[] = [
  { value: 'open', label: 'Open' },
  { value: 'pending', label: 'Pending' },
  { value: 'resolved', label: 'Resolved' },
  { value: 'closed', label: 'Closed' },
];

@Component({
  selector: 'app-conversation-detail',
  imports: [
    AvatarComponent,
    ChannelBadgeComponent,
    ComposerComponent,
    ConversationThreadComponent,
    EmptyStateComponent,
    AiHandlingBannerComponent,
    ConversationSummaryComponent,
    EscalationBannerComponent,
    LoadingStateComponent,
    SelectFilterComponent,
    StatusBadgeComponent,
  ],
  providers: [],
  template: `
    @if (store.loading()) {
      <app-loading-state />
    } @else if (store.error(); as err) {
      <app-empty-state icon="@tui.alert-circle" title="Something went wrong" [description]="err">
        <button type="button" (click)="goBack()">Go back</button>
      </app-empty-state>
    } @else if (store.conversation(); as conv) {
      <section class="detail-shell">
        <header class="detail-header">
          <div class="header-left">
            <button type="button" class="back-btn" (click)="goBack()">&larr; Back</button>
            <app-avatar [initials]="customerInitials(conv)" size="md" />
            <div class="header-copy">
              <strong>{{ conv.customer.displayName }}</strong>
              <span class="header-channel">
                <app-channel-badge [channel]="channelBadge(conv.channel)" />
                <app-status-badge [status]="conv.status" [tone]="statusTone(conv.status)" />
                @if (conv.widgetInstance; as wgt) {
                  <span class="widget-badge">{{ wgt.name }}</span>
                }
              </span>
            </div>
          </div>
          @if (hasManagePerm()) {
            <div class="header-right">
              <div class="control-group">
                <span class="control-label">Status</span>
                <app-select-filter
                  label="Status"
                  [value]="conv.status"
                  [options]="statusOptions"
                  (valueChange)="onStatusChange(conv.id, $event)"
                />
              </div>
              <div class="control-group">
                <span class="control-label">Assignee</span>
                <app-select-filter
                  label="Assignee"
                  [value]="assigneeValue(conv)"
                  [options]="assigneeOptions()"
                  (valueChange)="onAssigneeChange(conv.id, $event)"
                />
              </div>
            </div>
          }
        </header>

        @if (conv.awaitingAiDecision) {
          <app-ai-handling-banner
            [visible]="true"
            (choosePlatformAi)="store.setAiHandling(conv.id, 'platform_ai')"
            (chooseHuman)="store.setAiHandling(conv.id, 'human')"
          />
        }

        @if (conv.escalation) {
          <app-escalation-banner [escalation]="conv.escalation" />
        }

        @if (conv.participants.length) {
          <div class="participants-bar">
            <span class="participants-label">Participants</span>
            @for (p of conv.participants; track p.type + (p.id ?? p.membershipId)) {
              <span class="participant-chip" [class.inactive]="p.active === false">
                {{ p.displayName }}
                <span class="participant-type">({{ p.type }})</span>
              </span>
            }
          </div>
        }

        @if (hasManagePerm()) {
          <app-conversation-summary [conversationId]="conv.id" />
        }

        <app-conversation-thread
          [messages]="store.timeline()"
          [loading]="store.loadingTimeline()"
          [hasMore]="store.hasMoreTimeline()"
          [activeGeneration]="store.activeGeneration()"
          [toolActivity]="store.toolActivity()"
          (loadOlder)="store.loadOlder(conv.id)"
        />

        @if (hasManagePerm()) {
          <app-composer
            [conversationId]="conv.id"
            [currentStatus]="conv.status"
            [submitting]="store.submitting()"
            (send)="store.addMessage(conv.id, $event)"
          />
        }
      </section>
    }
  `,
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
      .participant-chip.inactive {
        color: var(--app-text-3);
        text-decoration: line-through;
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

  protected readonly statusOptions = STATUS_OPTIONS;
  protected readonly members = toSignal(this.api.listAssignableMembers().pipe(map((r) => r.data)), {
    initialValue: [],
  });

  protected readonly assigneeOptions = toSignal(
    this.api.listAssignableMembers().pipe(
      map((members) => {
        const opts: SelectFilterOption[] = [{ value: '', label: 'Unassigned' }];
        for (const m of members.data) {
          opts.push({ value: m.id, label: m.displayName });
        }
        return opts;
      }),
    ),
    { initialValue: [{ value: '', label: 'Unassigned' }] },
  );

  protected readonly hasManagePerm = () =>
    this.permissions.has('conversations.manage' as Permission);

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
}
