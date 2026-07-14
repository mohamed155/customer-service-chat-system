import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { AvailabilityState } from '../../../core/api/tenant-api.models';

@Component({
  selector: 'app-availability-dot',
  imports: [TuiIcon],
  template: `
    <span
      class="dot"
      [class.available]="state() === 'available'"
      [class.away]="state() === 'away'"
      [attr.aria-label]="state() === 'available' ? 'Available' : 'Away'"
    >
      @if (state() === 'available') {
        <tui-icon icon="@tui.circle-check" />
      }
      @if (state() === 'away') {
        <tui-icon icon="@tui.circle-minus" />
      }
    </span>
  `,
  styles: [
    `
      .dot {
        display: inline-flex;
        align-items: center;
        gap: 0.25rem;
      }
      .available {
        color: var(--tui-status-positive);
      }
      .away {
        color: var(--tui-status-neutral);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AvailabilityDotComponent {
  readonly state = input.required<AvailabilityState>();
}
