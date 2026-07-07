import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { RouterLink, RouterLinkActive } from '@angular/router';
import { TuiIcon } from '@taiga-ui/core';

@Component({
  selector: 'app-sidebar-nav-item',
  imports: [RouterLink, RouterLinkActive, TuiIcon],
  host: { '[class.collapsed]': 'collapsed()' },
  template: `
    <a
      [routerLink]="link()"
      routerLinkActive="active"
      [routerLinkActiveOptions]="{ exact: true }"
      ariaCurrentWhenActive="page"
      [attr.aria-label]="collapsed() ? label() : null"
    >
      <tui-icon [icon]="icon()" />
      @if (!collapsed()) {
        <span class="label">{{ label() }}</span>
        @if (badgeCount()) {
          <span class="badge">{{ badgeCount() }}</span>
        }
      }
    </a>
  `,
  styles: [
    `
      :host {
        display: block;
        width: 100%;
      }
      a {
        height: 36px;
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        padding: 0 10px;
        border-radius: 9px;
        color: var(--app-text-2);
        text-decoration: none;
        font-size: var(--app-font-sm);
        font-weight: 600;
      }
      a:hover {
        background: var(--app-panel-2);
        color: var(--app-text);
      }
      a.active {
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      a:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      tui-icon {
        flex: 0 0 auto;
        font-size: 17px;
      }
      .label {
        min-width: 0;
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .badge {
        min-width: 19px;
        height: 19px;
        display: inline-grid;
        place-items: center;
        padding: 0 6px;
        border-radius: 999px;
        background: var(--app-red);
        color: white;
        font-size: 10px;
        font-weight: 700;
      }
      :host(.collapsed) a {
        width: 38px;
        justify-content: center;
        padding: 0;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SidebarNavItemComponent {
  readonly icon = input.required<string>();
  readonly label = input.required<string>();
  readonly link = input.required<string>();
  readonly collapsed = input(false);
  readonly badgeCount = input<number | undefined>();
}
