import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

export interface SelectFilterOption {
  value: string;
  label: string;
}

@Component({
  selector: 'app-select-filter',
  template: `
    <select [attr.aria-label]="label()" [value]="value()" (change)="onChange($event)">
      @for (option of options(); track option.value) {
        <option [value]="option.value">{{ option.label }}</option>
      }
    </select>
  `,
  styles: [
    `
      :host {
        display: inline-flex;
      }
      select {
        height: 38px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
        font: inherit;
        font-size: var(--app-font-sm);
      }
      select:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-color: var(--app-accent);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SelectFilterComponent {
  readonly label = input.required<string>();
  readonly value = input('all');
  readonly options = input.required<SelectFilterOption[]>();
  readonly valueChange = output<string>();

  protected onChange(event: Event): void {
    this.valueChange.emit((event.target as HTMLSelectElement).value);
  }
}
