import { ChangeDetectionStrategy, Component, input, model } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';

@Component({
  selector: 'app-search-input',
  imports: [TuiIcon],
  template: `
    <label>
      <span class="sr-only">{{ placeholder() }}</span>
      <tui-icon icon="@tui.search" />
      <input
        type="search"
        [placeholder]="placeholder()"
        [value]="value()"
        (input)="updateValue($event)"
      />
      @if (shortcutHint()) {
        <kbd>{{ shortcutHint() }}</kbd>
      }
    </label>
  `,
  styles: [
    `
      :host {
        display: block;
        min-width: 0;
      }
      label {
        height: 38px;
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text-3);
      }
      label:focus-within {
        border-color: var(--app-accent);
        box-shadow: 0 0 0 3px var(--app-accent-soft);
      }
      input {
        min-width: 0;
        flex: 1;
        border: 0;
        outline: 0;
        background: transparent;
        color: var(--app-text);
        font: inherit;
      }
      input::placeholder {
        color: var(--app-text-3);
      }
      tui-icon {
        font-size: 16px;
      }
      kbd {
        padding: 2px 6px;
        border-radius: var(--app-radius-xs);
        background: var(--app-panel);
        border: 1px solid var(--app-border);
        color: var(--app-text-3);
        font: 500 var(--app-font-xs) / 1 var(--app-font-mono);
      }
      .sr-only {
        position: absolute;
        width: 1px;
        height: 1px;
        overflow: hidden;
        clip: rect(0, 0, 0, 0);
        white-space: nowrap;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SearchInputComponent {
  readonly placeholder = input('Search');
  readonly shortcutHint = input<string | undefined>();
  readonly value = model('');

  protected updateValue(event: Event): void {
    this.value.set((event.target as HTMLInputElement).value);
  }
}
