import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterLink } from '@angular/router';

import { injectBreadcrumbs } from '../../core/router/breadcrumb';
import { TranslocoPipe } from '@jsverse/transloco';

@Component({
  selector: 'app-breadcrumb',
  imports: [RouterLink, TranslocoPipe],
  templateUrl: './breadcrumb.component.html',
  styles: [
    `
      nav {
        padding: var(--app-space-2) 0;
        font-size: var(--app-font-sm);
      }
      .breadcrumb-inner {
        max-width: var(--app-content-max-width);
        margin: 0 auto;
        padding: 0 var(--app-page-padding-x);
      }
      ol {
        display: flex;
        flex-wrap: wrap;
        gap: var(--app-space-2);
        row-gap: var(--app-space-1);
        list-style: none;
        margin: 0;
        padding: 0;
        align-items: center;
      }
      li {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      /* U+203A is a mirrored character: the browser draws it as ‹ in a right-to-left page. */
      li + li::before {
        content: '›';
        margin-inline-end: var(--app-space-2);
        color: var(--app-text-2);
      }
      a {
        color: var(--app-accent);
        text-decoration: none;
      }
      a:hover {
        text-decoration: underline;
      }
      span[aria-current='page'] {
        font-weight: 600;
        color: var(--app-text);
      }
      span:not([aria-current]) {
        color: var(--app-text-2);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BreadcrumbComponent {
  readonly crumbs = injectBreadcrumbs();
}
