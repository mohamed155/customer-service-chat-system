import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'app-page-header',
  templateUrl: './page-header.component.html',
  styles: [
    `
      .page-header {
        margin-bottom: var(--app-space-6);
      }
      .page-header > div {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-4);
      }
      .header-text {
        display: flex;
        flex-direction: column;
      }
      h1 {
        margin: 0;
        font-size: var(--app-font-xl);
      }
      .description {
        margin: var(--app-space-1) 0 0;
        font-size: var(--app-font-sm);
        color: var(--app-text-2);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PageHeaderComponent {
  readonly title = input.required<string>();
  readonly description = input<string>();
}
