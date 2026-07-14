import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { filter } from 'rxjs';
import { ApiService } from '../../core/api/api.service';
import { Availability, AvailabilityState } from '../../core/api/tenant-api.models';
import { RealtimeService } from '../../core/realtime/realtime.service';

@Component({
  selector: 'app-availability-toggle',
  imports: [TuiIcon],
  template: `
    <button
      type="button"
      class="toggle"
      (click)="toggle()"
      [class.available]="state() === 'available'"
      [class.away]="state() === 'away'"
    >
      <tui-icon [icon]="state() === 'available' ? '@tui.circle-check' : '@tui.circle-minus'" />
      <span>{{ state() === 'available' ? 'Available' : 'Away' }}</span>
    </button>
  `,
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
    this.api.get<Availability>('tenant/availability/me').subscribe((res) => {
      this.state.set(res.data.state);
    });
  }

  toggle(): void {
    const newState: AvailabilityState = this.state() === 'available' ? 'away' : 'available';
    this.api.put<Availability>('tenant/availability/me', { state: newState }).subscribe((res) => {
      this.state.set(res.data.state);
    });
  }
}
