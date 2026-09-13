import { ChangeDetectionStrategy, Component } from '@angular/core';

@Component({
  selector: 'app-page-container',
  templateUrl: './page-container.component.html',
  styles: [
    `
      div {
        max-width: var(--app-content-max-width);
        margin: 0 auto;
        padding: var(--app-page-padding-y) var(--app-page-padding-x);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PageContainerComponent {}
