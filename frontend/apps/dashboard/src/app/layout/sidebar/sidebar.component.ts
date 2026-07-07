import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { APP_PATHS } from '../../core/router/app-paths';
import { AvatarComponent } from '../../shared/components/avatar/avatar.component';
import { IconButtonComponent } from '../../shared/components/icon-button/icon-button.component';
import { SIDEBAR_USER } from '../../shared/fixtures/settings.fixtures';
import { SidebarNavGroupComponent } from './sidebar-nav-group.component';
import { SidebarNavItemComponent } from './sidebar-nav-item.component';

@Component({
  selector: 'app-sidebar',
  imports: [
    AvatarComponent,
    IconButtonComponent,
    SidebarNavGroupComponent,
    SidebarNavItemComponent,
    TuiIcon,
  ],
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
        <app-sidebar-nav-group label="Workspace" [collapsed]="collapsed()">
          <app-sidebar-nav-item
            icon="@tui.layout-dashboard"
            label="Overview"
            [link]="links.overview"
            [collapsed]="collapsed()"
          />
          <app-sidebar-nav-item
            icon="@tui.messages-square"
            label="Conversations"
            [link]="links.conversations"
            [collapsed]="collapsed()"
            [badgeCount]="6"
          />
          <app-sidebar-nav-item
            icon="@tui.users"
            label="Customers"
            [link]="links.customers"
            [collapsed]="collapsed()"
          />
        </app-sidebar-nav-group>

        <app-sidebar-nav-group label="AI" [collapsed]="collapsed()">
          <app-sidebar-nav-item
            icon="@tui.bot"
            label="AI Agent"
            [link]="links.aiAgent"
            [collapsed]="collapsed()"
          />
          <app-sidebar-nav-item
            icon="@tui.book-open"
            label="Knowledge Base"
            [link]="links.knowledgeBase"
            [collapsed]="collapsed()"
          />
          <app-sidebar-nav-item
            icon="@tui.plug"
            label="Integrations"
            [link]="links.integrations"
            [collapsed]="collapsed()"
          />
        </app-sidebar-nav-group>

        <app-sidebar-nav-group label="Insights" [collapsed]="collapsed()">
          <app-sidebar-nav-item
            icon="@tui.chart-line"
            label="Analytics"
            [link]="links.analytics"
            [collapsed]="collapsed()"
          />
        </app-sidebar-nav-group>

        <app-sidebar-nav-group label="Settings" [collapsed]="collapsed()">
          <app-sidebar-nav-item
            icon="@tui.settings"
            label="Settings"
            [link]="links.settings"
            [collapsed]="collapsed()"
          />
        </app-sidebar-nav-group>
      </nav>

      <footer>
        <app-avatar [initials]="user.avatarInitials" size="sm" />
        @if (!collapsed()) {
          <div>
            <strong>{{ user.name }}</strong>
            <span>{{ user.role }} · {{ user.company }}</span>
          </div>
          <app-icon-button icon="@tui.log-out" label="Log out" />
        }
      </footer>
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
      .brand strong,
      footer strong {
        display: block;
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 700;
      }
      .brand span:not(.logo),
      footer span {
        display: block;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      nav {
        display: grid;
        gap: var(--app-space-4);
      }
      footer {
        min-height: 54px;
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        margin-top: auto;
        padding: var(--app-space-3) var(--app-space-2);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
      }
      footer div {
        min-width: 0;
        flex: 1;
      }
      :host.collapsed aside {
        align-items: center;
        padding-inline: var(--app-space-2);
      }
      :host.collapsed .brand,
      :host.collapsed footer {
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
  protected readonly user = SIDEBAR_USER;
  protected readonly links = {
    overview: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.overview}`,
    conversations: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.conversations}`,
    customers: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.customers}`,
    aiAgent: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.aiAgent}`,
    knowledgeBase: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.knowledgeBase}`,
    integrations: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.integrations}`,
    analytics: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.analytics}`,
    settings: `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.settings}`,
  } as const;
}
