import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'app-inline-alert',
  templateUrl: './inline-alert.component.html',
  styles: [
    `
      p {
        margin: var(--app-space-2) 0;
        font-size: var(--app-font-sm);
        font-weight: 600;
      }
      .error {
        color: var(--app-red);
      }
      .info {
        color: var(--app-text-2);
      }
      .warning {
        color: var(--app-amber);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class InlineAlertComponent {
  readonly tone = input<'error' | 'info' | 'warning'>('info');
}
