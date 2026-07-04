import { Component, Input } from '@angular/core';
@Component({
  selector: 'hx-badge',
  standalone: true,
  template: `<span class="hx-badge hx-badge--{{ status }}"><ng-content /></span>`,
  styles: [
    `
      .hx-badge {
        background: var(--panel-3);
        color: var(--text-2);
        padding: 3px 7px;
      }
      .hx-badge--success {
        background: var(--green-soft);
        color: var(--green);
      }
      .hx-badge--warning {
        background: var(--amber-soft);
        color: var(--amber);
      }
      .hx-badge--danger {
        background: var(--red-soft);
        color: var(--red);
      }
    `,
  ],
})
export class BadgeComponent {
  @Input() status: 'neutral' | 'success' | 'warning' | 'danger' = 'neutral';
}
