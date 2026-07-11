import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterLink } from '@angular/router';

import { injectBreadcrumbs } from '../../core/router/breadcrumb';

@Component({
  selector: 'app-breadcrumb',
  imports: [RouterLink],
  template: `<nav aria-label="Breadcrumb">
    <div class="breadcrumb-inner">
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
    </div>
  </nav>`,
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
      li + li::before {
        content: '›';
        margin-right: var(--app-space-2);
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
