import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { Store } from '@ngrx/store';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { LocaleDatePipe, LocaleNumberPipe } from '../../../core/i18n/format';
import { roleNameKey } from '../../../core/i18n/labels';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PAGE_ROUTE, RoutedPageStore } from '../../../core/routed-page/routed-page.store';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { appUiActions, ThemeMode } from '../../../core/state/app-ui.feature';
import { SettingsStore, SettingsTab } from './settings.store';

@Component({
  selector: 'app-settings',
  imports: [
    AvatarComponent,
    DashboardCardComponent,
    DataTableComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    LocaleDatePipe,
    LocaleNumberPipe,
    PageContainerComponent,
    PageHeaderComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
    TranslocoPipe,
  ],
  providers: [
    SettingsStore,
    RoutedPageStore,
    { provide: PAGE_ROUTE, useValue: 'settings' },
    provideTranslocoScope('settings'),
  ],
  templateUrl: './settings.component.html',
  styles: [
    `
      .tabs,
      .segmented {
        display: flex;
        gap: var(--app-space-1);
        margin-bottom: var(--app-space-5);
        padding: 4px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
        width: fit-content;
      }
      button {
        height: 34px;
        padding: 0 var(--app-space-3);
        border: 0;
        border-radius: var(--app-radius-md);
        background: transparent;
        color: var(--app-text-2);
        font-weight: 650;
        cursor: pointer;
      }
      button.active,
      .segmented button:hover {
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      .grid {
        display: grid;
        gap: var(--app-space-4);
      }
      .two {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }
      label {
        display: grid;
        gap: 6px;
        margin-bottom: var(--app-space-3);
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        font-weight: 650;
      }
      input {
        height: 38px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
        font: inherit;
      }
      .mono {
        font-family: var(--app-font-mono);
      }
      .check,
      .switch {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
      }
      .check input,
      .switch input {
        width: auto;
        height: auto;
      }
      .person {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
      }
      small {
        display: block;
        color: var(--app-text-3);
      }
      .usage {
        display: grid;
        gap: 7px;
        margin-bottom: var(--app-space-4);
      }
      .usage span {
        color: var(--app-text-2);
      }
      .usage div {
        height: 8px;
        overflow: hidden;
        border-radius: 999px;
        background: var(--app-panel-3);
      }
      .usage i {
        display: block;
        height: 100%;
        background: var(--app-accent);
      }
      .current {
        margin-left: 8px;
        color: var(--app-green);
        font-size: var(--app-font-xs);
        font-weight: 700;
      }
      @media (max-width: 900px) {
        .two {
          grid-template-columns: 1fr;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SettingsComponent {
  protected readonly page = inject(RoutedPageStore);
  protected readonly hasData = computed(() => this.page.data() !== undefined);
  protected readonly hasError = computed(() => this.page.error() !== null);
  private readonly globalStore = inject(Store);
  protected readonly store = inject(SettingsStore);
  private readonly locale = inject(LocaleService);
  protected readonly roleNameKey = roleNameKey;
  protected readonly themeModes: readonly { id: ThemeMode; key: string }[] = [
    { id: 'light', key: marker('settings.theme.light') },
    { id: 'dark', key: marker('settings.theme.dark') },
    { id: 'system', key: marker('settings.theme.system') },
  ];
  protected readonly tabs: readonly { id: SettingsTab; key: string }[] = [
    { id: 'general', key: marker('settings.tabs.general') },
    { id: 'team', key: marker('settings.tabs.team') },
    { id: 'billing', key: marker('settings.tabs.billing') },
    { id: 'api-keys', key: marker('settings.tabs.apiKeys') },
    { id: 'security', key: marker('settings.tabs.security') },
  ];

  protected readonly pageData = computed(() => {
    const data = this.page.data();
    if (data?.page === 'settings') return data.data;
    return undefined;
  });

  /** The name from the catalog, or the data's own word for a value this app has no name for. */
  protected nameOf(key: string | null, fallback: string): string {
    return key ? this.locale.translate(key) : fallback;
  }

  protected memberStatusKey(status: string): string | null {
    return status === 'active'
      ? marker('status.active')
      : status === 'invited'
        ? marker('settings.team.invited')
        : null;
  }

  protected setTheme(themeMode: ThemeMode): void {
    this.globalStore.dispatch(appUiActions.themeModeChanged({ themeMode }));
  }

  protected retry(): void {
    this.page.retry();
  }
}
