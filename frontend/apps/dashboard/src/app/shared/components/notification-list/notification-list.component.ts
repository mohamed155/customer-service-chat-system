import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { NotificationEntry } from '../../../core/api/tenant-api.models';
import { EmptyStateComponent } from '../empty-state/empty-state.component';
import { LoadingStateComponent } from '../loading-state/loading-state.component';
import { RelativeTimePipe } from '../../../core/i18n/relative-time.pipe';
import { TranslocoPipe } from '@jsverse/transloco';

function stateClass(state: string): string {
  if (state === 'unread') return 'state-unread';
  if (state === 'resolved') return 'state-resolved';
  return 'state-read';
}

@Component({
  selector: 'app-notification-list',
  imports: [RelativeTimePipe, TuiIcon, EmptyStateComponent, LoadingStateComponent, TranslocoPipe],
  templateUrl: './notification-list.component.html',
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
