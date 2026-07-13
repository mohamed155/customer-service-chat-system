import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'app-inline-alert',
  template: `<p
    [class]="tone()"
    [attr.role]="tone() === 'error' ? 'alert' : 'status'"
    [attr.aria-live]="tone() === 'error' ? 'assertive' : 'polite'"
  >
    <ng-content />
  </p>`,
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
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class InlineAlertComponent {
  readonly tone = input<'error' | 'info'>('info');
}
