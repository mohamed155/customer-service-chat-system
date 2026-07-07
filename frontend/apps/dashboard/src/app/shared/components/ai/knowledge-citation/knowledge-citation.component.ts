import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';

@Component({
  selector: 'app-knowledge-citation',
  imports: [TuiIcon],
  template: `
    @for (title of titles(); track title) {
      <span><tui-icon icon="@tui.book-open" />{{ title }}</span>
    }
  `,
  styles: [
    `
      :host {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        flex-wrap: wrap;
      }
      span {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 4px 7px;
        border: 1px solid var(--app-border);
        border-radius: 999px;
        background: var(--app-panel-2);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
      }
      tui-icon {
        font-size: 12px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class KnowledgeCitationComponent {
  readonly titles = input.required<readonly string[]>();
}
