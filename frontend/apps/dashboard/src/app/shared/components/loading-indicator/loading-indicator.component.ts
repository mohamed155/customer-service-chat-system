import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { TuiLoader } from '@taiga-ui/core';
/** Keep request loading local: `readonly loading = signal(false)` per operation. */
@Component({
  selector: 'app-loading-indicator',
  imports: [TuiLoader],
  template: `<div role="status" aria-live="polite" aria-busy="true">
    <tui-loader [showLoader]="true" [size]="size()" /><span class="label">Loading</span>
  </div>`,
  styles: [
    `
      .label {
        position: absolute;
        width: 1px;
        height: 1px;
        overflow: hidden;
        clip-path: inset(50%);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LoadingIndicatorComponent {
  readonly size = input<'s' | 'm' | 'l' | 'xl'>('m');
}
