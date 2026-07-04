import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { Store } from '@ngrx/store';
import { TuiButton, TuiIcon } from '@taiga-ui/core';
import { APP_CONFIG } from '../../core/config/app-config';
import { appUiActions, selectSidebarCollapsed, ThemeMode } from '../../core/state/app-ui.feature';

@Component({
  selector: 'app-topbar',
  imports: [TuiButton, TuiIcon],
  template: `
    <header>
      <button
        tuiButton
        type="button"
        appearance="flat"
        aria-label="Toggle sidebar"
        [attr.aria-expanded]="!collapsed()"
        (click)="toggleSidebar()"
      >
        <tui-icon icon="@tui.menu" />
      </button>
      <strong>{{ config.appName }}</strong>
      <div class="tools" aria-label="Display preferences">
        @for (mode of themeModes; track mode) {
          <button
            tuiButton
            type="button"
            size="s"
            appearance="flat"
            [attr.aria-label]="'Use ' + mode + ' theme'"
            (click)="setTheme(mode)"
          >
            {{ mode }}
          </button>
        }
        <button tuiButton type="button" appearance="flat" aria-label="Notifications">
          <tui-icon icon="@tui.bell" />
        </button>
      </div>
    </header>
  `,
  styles: [
    `
      header {
        height: var(--app-topbar-height);
        display: flex;
        align-items: center;
        gap: var(--app-space-4);
        padding: 0 var(--app-space-6);
        background: var(--app-color-surface);
        border-bottom: 1px solid var(--app-color-border);
      }
      .tools {
        margin-left: auto;
        display: flex;
        align-items: center;
        gap: var(--app-space-1);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TopbarComponent {
  private readonly store = inject(Store);
  protected readonly config = inject(APP_CONFIG);
  protected readonly collapsed = this.store.selectSignal(selectSidebarCollapsed);
  protected readonly themeModes: readonly ThemeMode[] = ['light', 'dark', 'system'];
  protected toggleSidebar(): void {
    this.store.dispatch(appUiActions.sidebarToggled());
  }
  protected setTheme(themeMode: ThemeMode): void {
    this.store.dispatch(appUiActions.themeModeChanged({ themeMode }));
  }
}
