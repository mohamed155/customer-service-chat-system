import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { AvailabilityState } from '../../../core/api/tenant-api.models';

@Component({
  selector: 'app-availability-dot',
  imports: [TuiIcon],
  templateUrl: './availability-dot.component.html',
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
