import { ChangeDetectionStrategy, Component, input, model } from '@angular/core';

export interface ChoiceGroupOption {
  value: string;
  label: string;
}

@Component({
  selector: 'app-choice-group',
  template: `
    <div role="group" [attr.aria-label]="ariaLabel()">
      @for (option of options(); track option.value) {
        <button
          type="button"
          [attr.aria-pressed]="value() === option.value"
          [class.selected]="value() === option.value"
          (click)="value.set(option.value)"
        >
          {{ option.label }}
        </button>
      }
    </div>
  `,
  styles: [
    `
      div {
        display: flex;
        flex-wrap: wrap;
        gap: var(--app-space-2);
      }
      button {
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        cursor: pointer;
        font: inherit;
        font-size: var(--app-font-sm);
        font-weight: 600;
      }
      button:hover,
      button.selected {
        border-color: var(--app-accent);
        background: var(--app-accent-soft);
      }
      button.selected {
        color: var(--app-accent);
      }
      button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ChoiceGroupComponent {
  readonly options = input.required<ChoiceGroupOption[]>();
  readonly value = model.required<string>();
  readonly ariaLabel = input('Options');
}
