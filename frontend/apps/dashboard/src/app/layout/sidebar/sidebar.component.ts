import { ChangeDetectionStrategy, Component, inject, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { PAGE_PERMISSIONS } from '../../core/authz/permissions';
import { PermissionsService } from '../../core/authz/permissions.service';
import { APP_PATHS } from '../../core/router/app-paths';
import { SidebarNavGroupComponent } from './sidebar-nav-group.component';
import { SidebarNavItemComponent } from './sidebar-nav-item.component';

@Component({
  selector: 'app-sidebar',
  imports: [SidebarNavGroupComponent, SidebarNavItemComponent, TuiIcon],
  host: { '[class.collapsed]': 'collapsed()' },
  templateUrl: './sidebar.component.html',
  styles: [
    `
      :host {
        display: block;
        width: var(--app-sidebar-expanded-width);
        height: 100dvh;
        background: var(--app-sidebar);
        border-right: 1px solid var(--app-border);
        transition: width var(--app-transition-base);
      }
      :host.collapsed {
        width: var(--app-sidebar-collapsed-width);
      }
      @media (max-width: 767px) {
        :host,
        :host.collapsed {
          width: var(--app-sidebar-expanded-width);
        }
      }
      aside {
        height: 100%;
        display: flex;
        flex-direction: column;
        overflow-y: auto;
        padding: var(--app-space-4) var(--app-space-3);
      }
      .brand {
        min-height: 44px;
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        padding: 0 var(--app-space-1);
        margin-bottom: var(--app-space-5);
      }
      .logo {
        width: 30px;
        height: 30px;
        display: grid;
        place-items: center;
        border-radius: 9px;
        background: linear-gradient(135deg, var(--app-accent), var(--app-accent-strong));
        color: var(--app-accent-ink);
        box-shadow: var(--app-shadow);
      }
      .brand strong {
        display: block;
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 700;
      }
      .brand span:not(.logo) {
        display: block;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      nav {
        display: grid;
        gap: var(--app-space-4);
      }
      :host.collapsed aside {
        align-items: center;
        padding-inline: var(--app-space-2);
      }
      :host.collapsed .brand {
        justify-content: center;
        padding-inline: 0;
        border-color: transparent;
        background: transparent;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SidebarComponent {
  readonly collapsed = input(false);
  protected readonly permissionsService = inject(PermissionsService);
  protected readonly PAGE_PERMISSIONS = PAGE_PERMISSIONS;
  protected readonly APP_PATHS = APP_PATHS;
  protected readonly links = {
    overview: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.overview}`,
    conversations: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.conversations}`,
    customers: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.customers}`,
    escalations: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.escalations}`,
    aiAgent: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.aiAgent}`,
    aiTools: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.settingsTools}`,
    knowledgeBase: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.knowledgeBase}`,
    integrations: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.integrations}`,
    analytics: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.analytics}`,
    auditLogs: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.auditLogs}`,
    billing: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.billing}`,
    settings: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.settings}`,
    team: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.team}`,
    widgets: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.widgets}`,
    security: `/${APP_PATHS.security}`,
  } as const;
}
