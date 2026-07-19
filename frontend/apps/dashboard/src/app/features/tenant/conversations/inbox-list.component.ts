import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import {
  ChannelBadgeComponent,
  ChannelBadgeChannel,
} from '../../../shared/components/channel-badge/channel-badge.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { Conversation, ConversationListQuery } from '../../../core/api/tenant-api.models';

@Component({
  selector: 'app-inbox-list',
  imports: [AvatarComponent, ChannelBadgeComponent, StatusBadgeComponent],
  template: `
    <div class="filters" aria-label="Conversation status filters">
      @for (filter of filters; track filter) {
        <button
          type="button"
          [class.active]="statusFilter() === filter"
          (click)="filterChanged.emit(filter)"
        >
          {{ filterLabel(filter) }}
        </button>
      }
    </div>

    <div class="list">
      @for (conversation of conversations(); track conversation.id) {
        <button
          type="button"
          class="item"
          [class.selected]="selectedId() === conversation.id"
          (click)="selected.emit(conversation.id)"
        >
          <app-avatar [initials]="customerInitials(conversation)" size="md" />
          <span class="copy">
            <strong>{{ conversation.customer.displayName }}</strong>
            <span class="preview">{{ conversation.lastMessage?.preview ?? 'No messages' }}</span>
            <span class="badges">
              <app-channel-badge [channel]="channelBadge(conversation.channel)" />
              <app-status-badge
                [status]="conversation.status"
                [tone]="statusTone(conversation.status)"
              />
              @if (conversation.widgetInstance; as wgt) {
                <span class="widget-badge" title="Chat Widget: {{ wgt.name }}">Widget</span>
              }
            </span>
          </span>
          <span class="meta">
            @if (conversation.assignee; as assignee) {
              <span class="assignee" [class.inactive]="!assignee.active">
                {{ assignee.displayName }}
              </span>
            }
            <time>{{ relativeTime(conversation.lastActivityAt) }}</time>
          </span>
        </button>
      }
    </div>
  `,
  styles: [
    `
      :host {
        display: grid;
        grid-template-rows: auto 1fr;
        min-height: 0;
        border-right: 1px solid var(--app-border);
        background: var(--app-panel);
      }
      .filters {
        display: flex;
        gap: var(--app-space-1);
        padding: var(--app-space-3);
        border-bottom: 1px solid var(--app-border);
        overflow-x: auto;
      }
      .filters button {
        height: 30px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: 999px;
        background: var(--app-panel);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        font-weight: 700;
        cursor: pointer;
      }
      .filters button.active {
        background: var(--app-accent-soft);
        border-color: var(--app-accent);
        color: var(--app-accent-strong);
      }
      .list {
        min-height: 0;
        overflow-y: auto;
        padding: var(--app-space-2);
      }
      .item {
        width: 100%;
        display: grid;
        grid-template-columns: auto 1fr auto;
        gap: var(--app-space-3);
        align-items: flex-start;
        padding: var(--app-space-3);
        border: 0;
        border-radius: var(--app-radius-lg);
        background: transparent;
        color: inherit;
        text-align: left;
        cursor: pointer;
      }
      .item:hover,
      .item.selected {
        background: var(--app-panel-2);
      }
      .item.selected {
        box-shadow: inset 3px 0 0 var(--app-accent);
      }
      .item:focus-visible {
        outline: 3px solid var(--app-accent-soft);
      }
      .copy {
        min-width: 0;
        display: grid;
        gap: 6px;
      }
      strong {
        display: flex;
        align-items: center;
        gap: 7px;
        color: var(--app-text);
        font-size: var(--app-font-sm);
      }
      .preview {
        overflow: hidden;
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .badges {
        display: flex;
        gap: 5px;
        flex-wrap: wrap;
      }
      .meta {
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: 4px;
      }
      .assignee {
        font-size: var(--app-font-xs);
        color: var(--app-text-2);
        white-space: nowrap;
      }
      .assignee.inactive {
        color: var(--app-text-3);
        text-decoration: line-through;
      }
      time {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
        white-space: nowrap;
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
      @media (max-width: 768px) {
        :host {
          border-right: 0;
          border-bottom: 1px solid var(--app-border);
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class InboxListComponent {
  readonly conversations = input.required<readonly Conversation[]>();
  readonly selectedId = input<string | null>(null);
  readonly statusFilter = input.required<ConversationListQuery['status']>();
  readonly selected = output<string>();
  readonly filterChanged = output<ConversationListQuery['status']>();
  protected readonly filters: readonly ConversationListQuery['status'][] = [
    'all',
    'open',
    'pending',
    'resolved',
    'closed',
  ];

  protected channelBadge(channel: string): ChannelBadgeChannel {
    return channel as ChannelBadgeChannel;
  }

  protected customerInitials(conversation: Conversation): string {
    return conversation.customer.displayName
      .split(' ')
      .map((part) => part[0])
      .join('')
      .toUpperCase()
      .slice(0, 2);
  }

  protected filterLabel(filter: ConversationListQuery['status']): string {
    return filter === 'all'
      ? 'All'
      : (filter as string).replace(/\b\w/g, (character) => character.toUpperCase());
  }

  protected statusTone(status: Conversation['status']): 'green' | 'amber' | 'red' | 'neutral' {
    if (status === 'closed' || status === 'resolved') return 'green';
    if (status === 'pending') return 'red';
    if (status === 'open') return 'amber';
    return 'neutral';
  }

  protected relativeTime(iso: string): string {
    const hours = Math.max(1, Math.round((Date.now() - new Date(iso).getTime()) / 3_600_000));
    return hours < 24 ? `${hours}h` : `${Math.round(hours / 24)}d`;
  }
}
