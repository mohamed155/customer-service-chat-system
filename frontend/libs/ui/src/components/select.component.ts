import { Component, Input } from '@angular/core';
@Component({
  selector: 'hx-select',
  standalone: true,
  template: `<label class="hx-select"
    ><span class="hx-select__label">{{ label }}</span
    ><select class="hx-select__control" [attr.aria-label]="label">
      <ng-content /></select
  ></label>`,
  styles: [
    `
      .hx-select {
        color: var(--text);
        display: grid;
        gap: 4px;
      }
      .hx-select__label {
        color: var(--text-2);
      }
      .hx-select__control {
        background: var(--panel);
        border: 1px solid var(--border-strong);
        color: var(--text);
        padding: 8px;
      }
    `,
  ],
})
export class SelectComponent {
  @Input() label = 'Select';
}
