import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'app-form-field',
  templateUrl: './form-field.component.html',
  styles: [
    `
      .field {
        display: grid;
        gap: var(--app-space-2);
      }
      label {
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 700;
      }
      ::ng-deep input,
      ::ng-deep select {
        width: 100%;
        box-sizing: border-box;
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font: inherit;
      }
      ::ng-deep input:focus-visible,
      ::ng-deep select:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-color: var(--app-accent);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class FormFieldComponent {
  readonly label = input.required<string>();
  readonly for = input<string>();
}
