import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'app-section-header',
  template: `
    <div>
      <h2>{{ title() }}</h2>
      @if (subtitle()) {
        <p>{{ subtitle() }}</p>
      }
    </div>
    <div class="actions"><ng-content /></div>
  `,
  styles: [
    `
      :host {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: var(--app-space-4);
        margin-bottom: var(--app-space-4);
      }
      h2 {
        margin: 0;
        color: var(--app-text);
        font-size: var(--app-font-lg);
        font-weight: 650;
      }
      p {
        margin: 4px 0 0;
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
      }
      .actions {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
      }
      .actions:empty {
        display: none;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SectionHeaderComponent {
  readonly title = input.required<string>();
  readonly subtitle = input<string | undefined>();
}
