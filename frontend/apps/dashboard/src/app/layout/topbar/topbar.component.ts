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
import { IconButtonComponent } from '../../shared/components/icon-button/icon-button.component';
import { SearchInputComponent } from '../../shared/components/search-input/search-input.component';
import { TenantSwitcherComponent } from './tenant-switcher.component';
import { UserMenuComponent } from './user-menu.component';

@Component({
  selector: 'app-topbar',
  imports: [
    IconButtonComponent,
    SearchInputComponent,
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
          <app-tenant-switcher />
        }
        <app-icon-button [icon]="themeIcon()" [label]="themeLabel()" (click)="cycleTheme()" />
        <app-icon-button icon="@tui.bell" label="Notifications" />
        @if (isAuthenticated()) {
          <app-user-menu />
        }
        <button class="new-button" type="button"><tui-icon icon="@tui.plus" />New</button>
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
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TopbarComponent {
  private readonly store = inject(Store);
  private readonly currentUser = inject(CurrentUserService);
  protected readonly collapsed = this.store.selectSignal(selectSidebarCollapsed);
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
    this.store.dispatch(appUiActions.sidebarToggled());
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
