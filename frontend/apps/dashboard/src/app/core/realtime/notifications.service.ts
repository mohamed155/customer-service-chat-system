import { inject, Injectable } from '@angular/core';
import { filter } from 'rxjs';
import { NotificationsStore } from '../notifications/notifications.store';
import { RealtimeService, SseEvent } from './realtime.service';

interface NotificationCreatedPayload {
  readonly membershipId: string;
  readonly notificationId: string;
  readonly unreadCount: number;
}

interface NotificationClearedPayload {
  readonly membershipId: string;
  readonly unreadCount: number;
}

@Injectable({ providedIn: 'root' })
export class NotificationsService {
  private readonly realtime = inject(RealtimeService);
  private readonly store = inject(NotificationsStore);

  requestPermission(): void {
    if (typeof Notification !== 'undefined' && Notification.permission === 'default') {
      void Notification.requestPermission();
    }
  }

  constructor() {
    this.realtime
      .events()
      .pipe(
        filter(
          (e): e is SseEvent & { event: 'notification.created' } =>
            e.event === 'notification.created',
        ),
      )
      .subscribe((event) => {
        const payload = JSON.parse(event.data) as NotificationCreatedPayload;
        this.store.setUnreadCount(payload.unreadCount);
        this.store.loadFirstPage();

        if (
          typeof Notification !== 'undefined' &&
          Notification.permission === 'granted' &&
          document.hidden
        ) {
          try {
            new Notification('New notification', {
              body: payload.notificationId,
            });
          } catch {
            new Notification('New notification');
          }
        }
      });

    this.realtime
      .events()
      .pipe(
        filter(
          (e): e is SseEvent & { event: 'notification.cleared' } =>
            e.event === 'notification.cleared',
        ),
      )
      .subscribe((event) => {
        const payload = JSON.parse(event.data) as NotificationClearedPayload;
        this.store.setUnreadCount(payload.unreadCount);
      });
  }
}
