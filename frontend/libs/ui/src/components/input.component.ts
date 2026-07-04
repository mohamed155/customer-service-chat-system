import { Component, Input } from '@angular/core';
@Component({
  selector: 'hx-input',
  standalone: true,
  template: `<label class="hx-input"
    ><span class="hx-input__label">{{ label }}</span
    ><input class="hx-input__control" [attr.aria-label]="label"
  /></label>`,
  styles: [
    `
      .hx-input {
        color: var(--text);
        display: grid;
        gap: 4px;
      }
      .hx-input__label {
        color: var(--text-2);
      }
      .hx-input__control {
        background: var(--panel);
        border: 1px solid var(--border-strong);
        color: var(--text);
        padding: 8px;
      }
    `,
  ],
})
export class InputComponent {
  @Input() label = 'Input';
}
