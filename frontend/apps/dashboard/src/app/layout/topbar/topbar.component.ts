import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';

import { Store } from '@ngrx/store';
import { TuiIcon } from '@taiga-ui/core';
import { injectPageTitle } from '../../core/router/page-title';
import { CurrentUserService } from '../../core/tenant/current-user.service';
import {
  appUiActions,
  selectSidebarCollapsed,
  selectThemeMode,
  ThemeMode,
} from '../../core/state/app-ui.feature';
import { PermissionsService } from '../../core/authz/permissions.service';
import { NotificationsService } from '../../core/realtime/notifications.service';
import { IconButtonComponent } from '../../shared/components/icon-button/icon-button.component';
import { SearchInputComponent } from '../../shared/components/search-input/search-input.component';
import { LayoutStore } from '../app-shell/layout.store';
import { AvailabilityToggleComponent } from './availability-toggle.component';
import { PlatformNavComponent } from './platform-nav.component';
import { TenantSwitcherComponent } from './tenant-switcher.component';
import { UserMenuComponent } from './user-menu.component';

@Component({
  selector: 'app-topbar',
  imports: [
    AvailabilityToggleComponent,
    IconButtonComponent,
    SearchInputComponent,
    PlatformNavComponent,
    TenantSwitcherComponent,
    UserMenuComponent,
    TuiIcon,
  ],
  template: `
    <header>
      <app-icon-button
        icon="@tui.menu"
        label="Toggle sidebar"
        [active]="collapsed()"
        [attr.aria-expanded]="!collapsed()"
        (click)="toggleSidebar()"
      />

      <div class="title">
        <strong>{{ pageTitle()?.title ?? 'Helix' }}</strong>
        <span>{{ pageTitle()?.subtitle ?? 'Support AI' }}</span>
      </div>

      <div class="tools" aria-label="Dashboard tools">
        <app-search-input
          class="search"
          placeholder="Search conversations, customers..."
          shortcutHint="⌘K"
          [(value)]="search"
        />
        @if (isPlatformUser()) {
          <app-platform-nav />
          <app-tenant-switcher />
        }
        <button class="new-button" type="button">
          <tui-icon icon="@tui.plus" /><span class="new-label">New</span>
        </button>
        @if (canManageConversations()) {
          <app-availability-toggle />
        }
        <app-icon-button
          class="theme-toggle"
          [icon]="themeIcon()"
          [label]="themeLabel()"
          (click)="cycleTheme()"
        />
        <div class="notification-wrapper">
          <app-icon-button class="notification-bell" icon="@tui.bell" label="Notifications" />
          @if (notificationsService.inAppSignal()) {
            <span class="badge">{{ notificationsService.inAppSignal() }}</span>
          }
        </div>
        @if (isAuthenticated()) {
          <app-user-menu />
        }
      </div>
    </header>
  `,
  styles: [
    `
      header {
        height: var(--app-topbar-height);
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        padding: 0 var(--app-page-padding-x);
        background: var(--app-panel);
        border-bottom: 1px solid var(--app-border);
      }
      .title {
        min-width: 0;
      }
      .title strong {
        display: block;
        color: var(--app-text);
        font-size: var(--app-font-lg);
        font-weight: 650;
        line-height: 1.1;
      }
      .title span {
        display: block;
        margin-top: 2px;
        color: var(--app-text-3);
        font-size: 11.5px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .tools {
        min-width: 0;
        margin-left: auto;
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
      }
      .search {
        width: 260px;
      }
      .new-button {
        height: 38px;
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-accent);
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 700;
        cursor: pointer;
      }
      .new-button:hover {
        border-color: var(--app-accent-strong);
        background: var(--app-accent-strong);
      }
      .new-button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      .notification-wrapper {
        position: relative;
        display: inline-flex;
      }
      .badge {
        position: absolute;
        top: -4px;
        right: -4px;
        min-width: 16px;
        height: 16px;
        padding: 0 4px;
        border-radius: 999px;
        background: var(--tui-status-danger, #dc2626);
        color: #fff;
        font-size: 10px;
        font-weight: 700;
        line-height: 16px;
        text-align: center;
        pointer-events: none;
      }
      @media (max-width: 900px) {
        .search {
          width: min(220px, 28vw);
        }
      }
      @media (max-width: 768px) {
        .search {
          display: none;
        }
      }
      @media (max-width: 480px) {
        header {
          gap: var(--app-space-2);
          padding: 0 var(--app-space-2);
        }
        .title {
          display: none;
        }
        .notification-bell {
          display: none;
        }
        .new-button {
          display: none;
        }
        .tools {
          gap: var(--app-space-1);
        }
        ::ng-deep app-platform-nav .trigger span,
        ::ng-deep app-tenant-switcher .trigger .name,
        ::ng-deep app-tenant-switcher .trigger tui-icon[icon='@tui.chevron-down'] {
          display: none;
        }
        ::ng-deep app-platform-nav .trigger,
        ::ng-deep app-tenant-switcher .trigger {
          padding: 0 var(--app-space-2);
          min-width: 38px;
          justify-content: center;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TopbarComponent {
  private readonly store = inject(Store);
  private readonly layoutStore = inject(LayoutStore);
  private readonly currentUser = inject(CurrentUserService);
  private readonly permissions = inject(PermissionsService);
  protected readonly notificationsService = inject(NotificationsService);
  protected readonly collapsed = this.store.selectSignal(selectSidebarCollapsed);
  protected readonly canManageConversations = () => this.permissions.has('conversations.manage');
  protected readonly isPlatformUser = this.currentUser.isPlatformUser;
  protected readonly isAuthenticated = computed(() => this.currentUser.currentUser() != null);
  protected readonly themeMode = this.store.selectSignal(selectThemeMode);
  protected readonly pageTitle = injectPageTitle();
  protected readonly search = signal('');
  protected readonly themeIcon = computed(() => {
    const mode = this.themeMode();
    return mode === 'light' ? '@tui.sun' : mode === 'dark' ? '@tui.moon' : '@tui.monitor';
  });
  protected readonly themeLabel = computed(() => {
    const current = this.themeMode();
    const next = this.nextThemeMode(current);
    return `Theme is ${current}; switch to ${next}`;
  });

  protected toggleSidebar(): void {
    if (this.layoutStore.isMobile()) {
      if (this.layoutStore.drawerOpen()) {
        this.layoutStore.closeDrawer();
      } else {
        this.layoutStore.openDrawer();
      }
    } else {
      this.store.dispatch(appUiActions.sidebarToggled());
    }
  }

  protected cycleTheme(): void {
    this.store.dispatch(
      appUiActions.themeModeChanged({ themeMode: this.nextThemeMode(this.themeMode()) }),
    );
  }

  private nextThemeMode(themeMode: ThemeMode): ThemeMode {
    return themeMode === 'light' ? 'dark' : themeMode === 'dark' ? 'system' : 'light';
  }
}
