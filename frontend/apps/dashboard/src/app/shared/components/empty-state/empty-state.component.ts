import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';

@Component({
  selector: 'app-empty-state',
  imports: [TuiIcon],
  template: `
    <div class="icon"><tui-icon [icon]="icon()" /></div>
    <h2>{{ title() }}</h2>
    <p>{{ description() }}</p>
    <div class="actions"><ng-content /></div>
  `,
  styles: [
    `
      :host {
        display: grid;
        justify-items: center;
        gap: var(--app-space-3);
        padding: var(--app-space-8);
        text-align: center;
        color: var(--app-text-2);
      }
      .icon {
        width: 42px;
        height: 42px;
        display: grid;
        place-items: center;
        border-radius: var(--app-radius-lg);
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      h2 {
        margin: 0;
        color: var(--app-text);
        font-size: var(--app-font-lg);
      }
      p {
        max-width: 420px;
        margin: 0;
        font-size: var(--app-font-sm);
      }
      .actions:empty {
        display: none;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class EmptyStateComponent {
  readonly icon = input.required<string>();
  readonly title = input.required<string>();
  readonly description = input.required<string>();
}
