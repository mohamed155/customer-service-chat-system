import { ChangeDetectionStrategy, Component } from '@angular/core';

@Component({
  selector: 'app-page-container',
  template: `<div>
    <ng-content />
  </div>`,
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
