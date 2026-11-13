import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { filter } from 'rxjs';
import { ApiService } from '../../core/api/api.service';
import type { components } from '../../core/api/generated/api';
import { AvailabilityState } from '../../core/api/tenant-api.models';
import { NotificationsService } from '../../core/realtime/notifications.service';
import { RealtimeService } from '../../core/realtime/realtime.service';
import { TranslocoPipe } from '@jsverse/transloco';

type Schemas = components['schemas'];

@Component({
  selector: 'app-availability-toggle',
  imports: [TuiIcon, TranslocoPipe],
  templateUrl: './availability-toggle.component.html',
  styles: [
    `
      .toggle {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        height: 34px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 600;
        cursor: pointer;
      }
      .toggle:hover {
        background: var(--app-panel-2);
      }
      .toggle.available {
        color: var(--tui-status-positive);
        border-color: var(--tui-status-positive);
      }
      .toggle.away {
        color: var(--tui-status-neutral);
        border-color: var(--tui-status-neutral);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AvailabilityToggleComponent {
  private readonly api = inject(ApiService);
  private readonly realtime = inject(RealtimeService, { optional: true });
  private readonly notifications = inject(NotificationsService);
  readonly state = signal<AvailabilityState>('away');

  constructor() {
    this.loadState();
    this.realtime
      ?.events()
      .pipe(filter((e) => e.event === 'availability.changed'))
      .subscribe((e) => {
        const data = JSON.parse(e.data);
        this.state.set(data.state);
      });
  }

  private loadState(): void {
    this.api.get<Schemas['Availability']>('tenant/availability/me').subscribe({
      next: (res) => this.state.set(res.data.state),
      error: () => {},
    });
  }

  toggle(): void {
    const newState: AvailabilityState = this.state() === 'available' ? 'away' : 'available';
    if (newState === 'available') {
      this.notifications.requestPermission();
    }
    this.api
      .put<Schemas['Availability']>('tenant/availability/me', {
        state: newState,
      } satisfies Schemas['SetAvailabilityPayload'])
      .subscribe({
        next: (res) => this.state.set(res.data.state),
        error: () => {},
      });
  }
}
