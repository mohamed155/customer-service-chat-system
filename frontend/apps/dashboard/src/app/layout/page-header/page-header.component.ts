import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'app-page-header',
  template: `<header>
    <div>
      <h1>{{ title() }}</h1>
      <ng-content />
    </div>
  </header>`,
  styles: [
    `
      header {
        margin-bottom: var(--app-space-6);
      }
      header > div {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-4);
      }
      h1 {
        margin: 0;
        font-size: var(--app-text-xl);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PageHeaderComponent {
  readonly title = input.required<string>();
}
