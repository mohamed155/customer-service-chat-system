import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { RouterLink, RouterLinkActive } from '@angular/router';
import { TuiIcon } from '@taiga-ui/core';
import { APP_PATHS } from '../../core/router/app-paths';

@Component({
  selector: 'app-sidebar',
  imports: [RouterLink, RouterLinkActive, TuiIcon],
  host: { '[class.collapsed]': 'collapsed()' },
  template: `
    <nav aria-label="Primary navigation">
      <a
        [routerLink]="platformUrl"
        routerLinkActive="active"
        [attr.aria-label]="collapsed() ? 'Platform' : null"
      >
        <tui-icon icon="@tui.gauge" />
        @if (!collapsed()) {
          <span>Platform</span>
        }
      </a>
      <a
        [routerLink]="tenantUrl"
        routerLinkActive="active"
        [attr.aria-label]="collapsed() ? 'Tenant' : null"
      >
        <tui-icon icon="@tui.building-2" />
        @if (!collapsed()) {
          <span>Tenant</span>
        }
      </a>
    </nav>
  `,
  styles: [
    `
      :host {
        display: block;
        width: var(--app-sidebar-width);
        min-height: 100vh;
        background: var(--app-color-surface);
        border-right: 1px solid var(--app-color-border);
        transition: width 160ms ease;
      }
      :host.collapsed {
        width: var(--app-sidebar-collapsed-width);
      }
      nav {
        display: grid;
        gap: var(--app-space-2);
        padding: var(--app-space-4);
      }
      a {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        min-height: 44px;
        padding: var(--app-space-2);
        border-radius: var(--app-radius-sm);
        color: var(--app-color-text);
        text-decoration: none;
      }
      a.active {
        background: var(--app-color-bg);
        color: var(--app-color-accent);
      }
      a:focus-visible {
        outline: 3px solid var(--app-color-accent);
        outline-offset: 2px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SidebarComponent {
  readonly collapsed = input(false);
  protected readonly platformUrl = `/${APP_PATHS.platform.base}/${APP_PATHS.platform.overviewPlaceholder}`;
  protected readonly tenantUrl = `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.overviewPlaceholder}`;
}
