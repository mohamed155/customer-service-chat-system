import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { NotificationEntry } from '../../../core/api/tenant-api.models';
import { EmptyStateComponent } from '../empty-state/empty-state.component';
import { LoadingStateComponent } from '../loading-state/loading-state.component';
import { relativeTime } from '../../utils/relative-time';

function stateClass(state: string): string {
  if (state === 'unread') return 'state-unread';
  if (state === 'resolved') return 'state-resolved';
  return 'state-read';
}

@Component({
  selector: 'app-notification-list',
  imports: [TuiIcon, EmptyStateComponent, LoadingStateComponent],
  template: `
    @if (loading() && items().length === 0) {
      <app-loading-state />
    } @else if (items().length === 0) {
      <app-empty-state
        icon="@tui.bell-off"
        title="No notifications"
        description="You're all caught up."
      />
    } @else {
      <div class="list" role="listbox">
        @for (item of items(); track item.id) {
          <div
            class="item {{ stateClass(item.state) }}"
            role="option"
            [attr.aria-selected]="false"
            tabindex="0"
            (click)="handleItemClick(item)"
            (keydown.enter)="handleItemClick(item)"
          >
            <div class="item-content">
              <div class="item-header">
                <span class="item-title">{{ item.title }}</span>
                <span class="item-time">{{ relativeTime(item.createdAt) }}</span>
              </div>
              @if (item.body; as body) {
                <p class="item-body">{{ body }}</p>
              }
              @if (item.state === 'unread') {
                <button
                  type="button"
                  class="mark-read-btn"
                  (click)="$event.stopPropagation(); markRead.emit(item.id)"
                >
                  <tui-icon icon="@tui.check" />
                  Mark read
                </button>
              }
            </div>
            <tui-icon class="item-icon" [icon]="iconForKind(item.kind)" />
          </div>
        }
      </div>
      @if (hasMore()) {
        <button type="button" class="load-more" (click)="loadMore.emit()">Load more</button>
      }
    }
  `,
  styles: [
    `
      .list {
        max-height: 420px;
        overflow-y: auto;
      }
      .item {
        display: flex;
        gap: var(--app-space-3);
        padding: var(--app-space-3);
        cursor: pointer;
        border-bottom: 1px solid var(--app-border);
        transition: background var(--app-transition-fast);
      }
      .item:last-child {
        border-bottom: none;
      }
      .item:hover {
        background: var(--app-panel-2);
      }
      .item:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: -3px;
      }
      .state-unread {
        background: var(--app-accent-soft, #eef2ff);
      }
      .state-resolved {
        opacity: 0.6;
      }
      .item-content {
        flex: 1;
        min-width: 0;
      }
      .item-header {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
        gap: var(--app-space-2);
      }
      .item-title {
        font-weight: 600;
        font-size: var(--app-font-sm);
        color: var(--app-text);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .item-time {
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
        white-space: nowrap;
      }
      .item-body {
        margin: 2px 0 0;
        font-size: var(--app-font-xs);
        color: var(--app-text-2);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .mark-read-btn {
        display: inline-flex;
        align-items: center;
        gap: 4px;
        margin-top: var(--app-space-2);
        padding: 2px var(--app-space-2);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-sm);
        background: var(--app-panel);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        cursor: pointer;
      }
      .mark-read-btn:hover {
        background: var(--app-panel-2);
        color: var(--app-text);
      }
      .item-icon {
        font-size: 18px;
        color: var(--app-text-3);
        flex-shrink: 0;
      }
      .load-more {
        width: 100%;
        padding: var(--app-space-3);
        border: none;
        border-top: 1px solid var(--app-border);
        background: var(--app-panel);
        color: var(--app-accent);
        font-size: var(--app-font-sm);
        font-weight: 600;
        cursor: pointer;
        text-align: center;
      }
      .load-more:hover {
        background: var(--app-panel-2);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NotificationListComponent {
  readonly items = input.required<NotificationEntry[]>();
  readonly loading = input(false);
  readonly hasMore = input(false);
  readonly itemClick = output<NotificationEntry>();
  readonly markRead = output<string>();
  readonly loadMore = output<void>();

  protected relativeTime = relativeTime;
  protected stateClass = stateClass;

  protected iconForKind(kind: string): string {
    if (kind.startsWith('escalation')) return '@tui.alert-triangle';
    if (kind.startsWith('tool_request')) return '@tui.terminal';
    if (kind.startsWith('ai.')) return '@tui.bot';
    return '@tui.message-square';
  }

  protected handleItemClick(item: NotificationEntry): void {
    this.itemClick.emit(item);
  }
}
