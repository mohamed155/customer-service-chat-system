import { Component, EventEmitter, Input, Output } from '@angular/core';
@Component({
  selector: 'hx-dialog',
  standalone: true,
  template: `@if (open) {
    <section
      class="hx-dialog"
      role="dialog"
      aria-modal="true"
      [attr.aria-label]="label"
      tabindex="-1"
      (keydown.escape)="close()"
    >
      <div class="hx-dialog__panel"><ng-content /></div>
    </section>
  }`,
  styles: [
    `
      .hx-dialog {
        background: var(--accent-soft);
        color: var(--text);
        inset: 0;
        position: fixed;
      }
      .hx-dialog__panel {
        background: var(--panel);
        border: 1px solid var(--border);
        box-shadow: var(--shadow-lg);
        margin: 10vh auto;
        padding: 16px;
        width: min(480px, 90vw);
      }
    `,
  ],
})
export class DialogComponent {
  @Input() open = false;
  @Input() label = 'Dialog';
  @Output() closed = new EventEmitter<void>();
  close() {
    this.closed.emit();
  }
}
