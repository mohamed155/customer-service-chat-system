import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterLink } from '@angular/router';

import { injectBreadcrumbs } from '../../core/router/breadcrumb';

@Component({
  selector: 'app-breadcrumb',
  imports: [RouterLink],
  template: `<nav aria-label="Breadcrumb">
    <ol>
      @for (crumb of crumbs(); track $index) {
        <li>
          @if (crumb.link; as link) {
            <a [routerLink]="[link]">{{ crumb.label }}</a>
          } @else {
            <span [attr.aria-current]="$last ? 'page' : undefined">{{ crumb.label }}</span>
          }
        </li>
      }
    </ol>
  </nav>`,
  styles: [
    `
      nav {
        padding: var(--app-space-2) var(--app-page-padding-x);
        font-size: var(--app-font-sm);
      }
      ol {
        display: flex;
        gap: var(--app-space-2);
        list-style: none;
        margin: 0;
        padding: 0;
        align-items: center;
      }
      li + li::before {
        content: '›';
        margin-right: var(--app-space-2);
        color: var(--app-text-secondary);
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
        color: var(--app-text-secondary);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BreadcrumbComponent {
  readonly crumbs = injectBreadcrumbs();
}
