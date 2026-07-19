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
  template: `
    <aside>
      <div class="brand" aria-label="Helix Support AI">
        <span class="logo"><tui-icon icon="@tui.sparkles" /></span>
        @if (!collapsed()) {
          <div>
            <strong>Helix</strong>
            <span>Support AI</span>
          </div>
        }
      </div>

      <nav aria-label="Primary navigation">
        @if (
          permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.overview]) ||
          permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.conversations]) ||
          permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.escalations]) ||
          permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.customers]) ||
          permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.team])
        ) {
          <app-sidebar-nav-group label="Workspace" [collapsed]="collapsed()">
            @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.overview])) {
              <app-sidebar-nav-item
                icon="@tui.layout-dashboard"
                label="Overview"
                [link]="links.overview"
                [collapsed]="collapsed()"
              />
            }
            @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.conversations])) {
              <app-sidebar-nav-item
                icon="@tui.messages-square"
                label="Conversations"
                [link]="links.conversations"
                [collapsed]="collapsed()"
                [badgeCount]="6"
              />
            }
            @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.escalations])) {
              <app-sidebar-nav-item
                icon="@tui.arrow-up-from-line"
                label="Escalations"
                [link]="links.escalations"
                [collapsed]="collapsed()"
              />
            }
            @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.customers])) {
              <app-sidebar-nav-item
                icon="@tui.users"
                label="Customers"
                [link]="links.customers"
                [collapsed]="collapsed()"
              />
            }
            @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.team])) {
              <app-sidebar-nav-item
                icon="@tui.badge-check"
                label="Team"
                [link]="links.team"
                [collapsed]="collapsed()"
              />
            }
          </app-sidebar-nav-group>
        }

        @if (
          permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.aiAgent]) ||
          permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.knowledgeBase]) ||
          permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.integrations]) ||
          permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.widgets])
        ) {
          <app-sidebar-nav-group label="AI" [collapsed]="collapsed()">
            @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.aiAgent])) {
              <app-sidebar-nav-item
                icon="@tui.bot"
                label="AI Agent"
                [link]="links.aiAgent"
                [collapsed]="collapsed()"
              />
            }
            @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.knowledgeBase])) {
              <app-sidebar-nav-item
                icon="@tui.book-open"
                label="Knowledge Base"
                [link]="links.knowledgeBase"
                [collapsed]="collapsed()"
              />
            }
            @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.integrations])) {
              <app-sidebar-nav-item
                icon="@tui.plug"
                label="Integrations"
                [link]="links.integrations"
                [collapsed]="collapsed()"
              />
            }
            @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.widgets])) {
              <app-sidebar-nav-item
                icon="@tui.message-square"
                label="Chat Widget"
                [link]="links.widgets"
                [collapsed]="collapsed()"
              />
            }
          </app-sidebar-nav-group>
        }

        @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.analytics])) {
          <app-sidebar-nav-group label="Insights" [collapsed]="collapsed()">
            <app-sidebar-nav-item
              icon="@tui.chart-line"
              label="Analytics"
              [link]="links.analytics"
              [collapsed]="collapsed()"
            />
          </app-sidebar-nav-group>
        }

        @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.settings])) {
          <app-sidebar-nav-group label="Settings" [collapsed]="collapsed()">
            <app-sidebar-nav-item
              icon="@tui.settings"
              label="Settings"
              [link]="links.settings"
              [collapsed]="collapsed()"
            />
          </app-sidebar-nav-group>
        }
      </nav>
    </aside>
  `,
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
    knowledgeBase: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.knowledgeBase}`,
    integrations: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.integrations}`,
    analytics: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.analytics}`,
    settings: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.settings}`,
    team: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.team}`,
    widgets: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.widgets}`,
  } as const;
}
