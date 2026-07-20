import { ChangeDetectionStrategy, Component, inject, OnInit } from '@angular/core';
import { Router } from '@angular/router';
import { TuiIcon } from '@taiga-ui/core';
import { APP_PATHS } from '../../../core/router/app-paths';
import { NotificationsStore } from '../../../core/notifications/notifications.store';
import { NotificationEntry } from '../../../core/api/tenant-api.models';
import { NotificationListComponent } from '../../../shared/components/notification-list/notification-list.component';

@Component({
  selector: 'app-notifications-page',
  imports: [TuiIcon, NotificationListComponent],
  template: `
    <div class="page">
      <div class="page-header">
        <h2>Notifications</h2>
        @if (store.unreadCount() > 0) {
          <button type="button" class="mark-all-btn" (click)="store.markAllRead()">
            <tui-icon icon="@tui.check-check" />
            Mark all as read
          </button>
        }
      </div>
      <div class="page-body">
        <app-notification-list
          [items]="store.items()"
          [loading]="store.loading()"
          [hasMore]="store.hasMore()"
          (itemClick)="onItemClick($event)"
          (markRead)="store.markRead($event)"
          (loadMore)="store.loadMore()"
        />
      </div>
    </div>
  `,
  styles: [
    `
      .page {
        padding: var(--app-page-padding-y) var(--app-page-padding-x);
      }
      .page-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-bottom: var(--app-space-4);
      }
      .page-header h2 {
        margin: 0;
        font-size: var(--app-font-xl);
        color: var(--app-text);
      }
      .mark-all-btn {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-2);
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        font-weight: 600;
        cursor: pointer;
      }
      .mark-all-btn:hover {
        background: var(--app-panel-2);
        color: var(--app-text);
      }
      .mark-all-btn tui-icon {
        font-size: 16px;
      }
      .page-body {
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
        overflow: hidden;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NotificationsPageComponent implements OnInit {
  private readonly router = inject(Router);
  protected readonly store = inject(NotificationsStore);

  ngOnInit(): void {
    this.store.loadFirstPage();
  }

  protected onItemClick(notification: NotificationEntry): void {
    if (notification.state === 'unread') {
      this.store.markRead(notification.id);
    }
    const route = this.routeForSubject(notification.subjectType, notification.subjectId);
    if (route) {
      void this.router.navigateByUrl(route);
    }
  }

  private routeForSubject(subjectType: string, subjectId: string): string | null {
    switch (subjectType) {
      case 'conversation':
      case 'escalation':
      case 'tool_request':
        return `/tenant/${APP_PATHS.tenant.conversationDetail(subjectId)}`;
      default:
        return null;
    }
  }
}
