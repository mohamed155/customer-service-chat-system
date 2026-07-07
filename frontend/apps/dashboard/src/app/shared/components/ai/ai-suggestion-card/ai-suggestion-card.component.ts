import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';

@Component({
  selector: 'app-ai-suggestion-card',
  imports: [TuiIcon],
  template: `
    <div class="head"><tui-icon icon="@tui.sparkles" /><strong>Suggested reply</strong></div>
    <p>{{ suggestion() }}</p>
    <div class="actions"><ng-content /></div>
  `,
  styles: [
    `
      :host {
        display: block;
        padding: var(--app-space-4);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel-2);
      }
      .head {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        color: var(--app-accent-strong);
        font-size: var(--app-font-sm);
      }
      p {
        margin: var(--app-space-3) 0;
        color: var(--app-text);
        font-size: var(--app-font-sm);
        line-height: 1.5;
      }
      .actions {
        display: flex;
        gap: var(--app-space-2);
      }
      .actions:empty {
        display: none;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AiSuggestionCardComponent {
  readonly suggestion = input.required<string>();
}
