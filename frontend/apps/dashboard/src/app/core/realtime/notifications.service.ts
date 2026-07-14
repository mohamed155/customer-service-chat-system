import { inject, Injectable, signal } from '@angular/core';
import { RealtimeService } from './realtime.service';
import { filter } from 'rxjs';

@Injectable({ providedIn: 'root' })
export class NotificationsService {
  private readonly realtime = inject(RealtimeService);
  readonly inAppSignal = signal<number>(0);

  requestPermission(): void {
    if ('Notification' in window && Notification.permission === 'default') {
      void Notification.requestPermission();
    }
  }

  constructor() {
    this.realtime
      .events()
      .pipe(filter((e) => e.event === 'escalation.assigned'))
      .subscribe((event) => {
        this.inAppSignal.update((n) => n + 1);

        if (
          'Notification' in window &&
          Notification.permission === 'granted' &&
          document.hidden
        ) {
          try {
            const data = JSON.parse(event.data) as Record<string, unknown>;
            new Notification('Escalation assigned', {
              body: (data.reason as string) ?? 'A new escalation has been assigned to you.',
            });
          } catch {
            new Notification('Escalation assigned');
          }
        }
      });
  }
}
