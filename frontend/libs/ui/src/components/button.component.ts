import { Component, EventEmitter, Input, Output } from '@angular/core';
@Component({
  selector: 'hx-button',
  standalone: true,
  template: `<button
    type="button"
    class="hx-button hx-button--{{ variant }}"
    [attr.aria-label]="ariaLabel"
    (click)="activate()"
    (keydown.enter)="activate()"
    (keydown.space)="activate(); $event.preventDefault()"
  >
    <ng-content />
  </button>`,
  styles: [
    `
      .hx-button {
        border: 1px solid var(--border-strong);
        background: var(--panel);
        color: var(--text);
        padding: 8px 14px;
      }
      .hx-button--primary {
        background: var(--accent);
        border-color: var(--accent);
        color: var(--accent-ink);
      }
      .hx-button--secondary {
        background: var(--panel-2);
      }
      .hx-button--danger {
        background: var(--red);
        border-color: var(--red);
        color: var(--accent-ink);
      }
    `,
  ],
})
export class ButtonComponent {
  @Input() variant: 'primary' | 'secondary' | 'danger' = 'primary';
  @Input() ariaLabel = 'Button';
  @Output() pressed = new EventEmitter<void>();
  activate() {
    this.pressed.emit();
  }
}
