import { Component, Input } from '@angular/core';
@Component({
  selector: 'hx-toast',
  standalone: true,
  template: `<aside class="hx-toast" role="status" [attr.aria-label]="label">
    <ng-content />
  </aside>`,
  styles: [
    `
      .hx-toast {
        background: var(--panel);
        border: 1px solid var(--border);
        box-shadow: var(--shadow);
        color: var(--text);
        padding: 12px;
      }
    `,
  ],
})
export class ToastComponent {
  @Input() label = 'Notification';
}
