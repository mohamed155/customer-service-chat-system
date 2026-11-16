import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { Store } from '@ngrx/store';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { HasPermissionDirective } from '../../../core/authz/has-permission.directive';
import { marker } from '../../../core/i18n/marker';
import { APP_PATHS } from '../../../core/router/app-paths';
import { appUiActions, selectThemeMode, ThemeMode } from '../../../core/state/app-ui.feature';
import { selectActiveTenant } from '../../../core/state/tenant-context.feature';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';

/**
 * What the settings page really has: the dashboard's own theme (kept in this browser), the
 * workspace you are in, and the way to the pages where workspace settings live. The workspace
 * has no profile, API key or session API yet, so none of those is shown.
 */
@Component({
  selector: 'app-settings',
  imports: [
    DashboardCardComponent,
    HasPermissionDirective,
    PageContainerComponent,
    PageHeaderComponent,
    RouterLink,
    SectionHeaderComponent,
    TranslocoPipe,
  ],
  providers: [provideTranslocoScope('settings')],
  templateUrl: './settings.component.html',
  styles: [
    `
      .grid {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: var(--app-space-4);
      }
      .segmented {
        display: flex;
        gap: var(--app-space-1);
        padding: 4px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
        width: fit-content;
      }
      .segmented button {
        height: 34px;
        padding: 0 var(--app-space-3);
        border: 0;
        border-radius: var(--app-radius-md);
        background: transparent;
        color: var(--app-text-2);
        font-weight: 650;
        cursor: pointer;
      }
      .segmented button:hover,
      .segmented button.active {
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      dl {
        display: grid;
        grid-template-columns: auto 1fr;
        gap: var(--app-space-2) var(--app-space-4);
        margin: 0;
      }
      dt {
        color: var(--app-text-3);
      }
      dd {
        margin: 0;
        color: var(--app-text);
      }
      ul {
        display: grid;
        gap: var(--app-space-2);
        margin: 0;
        padding: 0;
        list-style: none;
      }
      li a {
        color: var(--app-accent-strong);
        font-weight: 650;
      }
      li small {
        display: block;
        color: var(--app-text-3);
      }
      @media (max-width: 900px) {
        .grid {
          grid-template-columns: 1fr;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SettingsComponent {
  private readonly store = inject(Store);
  protected readonly tenant = this.store.selectSignal(selectActiveTenant);
  protected readonly themeMode = this.store.selectSignal(selectThemeMode);

  protected readonly themeModes: readonly { id: ThemeMode; key: string }[] = [
    { id: 'light', key: marker('settings.theme.light') },
    { id: 'dark', key: marker('settings.theme.dark') },
    { id: 'system', key: marker('settings.theme.system') },
  ];

  protected readonly teamLink = `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.team}`;
  protected readonly billingLink = `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.billing}`;
  protected readonly toolsLink = `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.settingsTools}`;

  protected setTheme(themeMode: ThemeMode): void {
    this.store.dispatch(appUiActions.themeModeChanged({ themeMode }));
  }
}
