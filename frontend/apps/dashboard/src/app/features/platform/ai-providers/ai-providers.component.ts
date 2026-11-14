import { ChangeDetectionStrategy, Component, computed, inject, OnInit } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { ApiError } from '../../../core/api/api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { LocaleDatePipe, LocaleNumberPipe } from '../../../core/i18n/format';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { failureCategoryKey, providerVerdictKey } from '../../../core/i18n/platform-labels';
import { HealthIndicatorComponent } from '../../../shared/components/health-indicator/health-indicator.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import {
  SelectFilterComponent,
  SelectFilterOption,
} from '../../../shared/components/select-filter/select-filter.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { ProviderStatus } from '../../../core/api/platform-analytics.models';
import { AiProvidersStore } from './ai-providers.store';

@Component({
  selector: 'app-ai-providers',
  standalone: true,
  imports: [
    FormsModule,
    HealthIndicatorComponent,
    StatusBadgeComponent,
    DashboardCardComponent,
    DataTableComponent,
    EmptyStateComponent,
    InlineAlertComponent,
    LoadingStateComponent,
    SelectFilterComponent,
    PageContainerComponent,
    PageHeaderComponent,
    LocaleDatePipe,
    LocaleNumberPipe,
    TranslocoPipe,
  ],
  providers: [AiProvidersStore, provideTranslocoScope('platform')],
  templateUrl: './ai-providers.component.html',
  styles: [
    `
      .toolbar {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        margin-bottom: var(--app-space-4);
        flex-wrap: wrap;
      }
      .utc-note {
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
      }
      .cards {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
        gap: var(--app-space-3);
        margin-bottom: var(--app-space-5);
      }
      .prov-row {
        display: flex;
        flex-direction: column;
        gap: var(--app-space-1);
      }
      .meta {
        margin: 0;
        font-size: var(--app-font-xs);
        color: var(--app-text-2);
      }
      .categories {
        margin-top: var(--app-space-2);
        font-size: var(--app-font-xs);
      }
      .categories summary {
        cursor: pointer;
        font-weight: 600;
      }
      .categories ul {
        margin: var(--app-space-1) 0 0;
        padding-left: var(--app-space-4);
      }
      .failures h3 {
        margin: 0 0 var(--app-space-2);
        font-size: var(--app-font-lg);
        font-weight: 650;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AiProvidersComponent implements OnInit {
  protected readonly store = inject(AiProvidersStore);
  private readonly locale = inject(LocaleService);
  protected readonly failureCategoryKey = failureCategoryKey;
  protected readonly providerVerdictKey = providerVerdictKey;
  protected readonly percentFormat = { style: 'percent', maximumFractionDigits: 1 } as const;

  protected readonly windowOptions = computed<SelectFilterOption[]>(() => [
    { value: '1h', label: this.locale.translate(marker('platform.providers.lastHour')) },
    { value: '24h', label: this.locale.translate(marker('platform.providers.last24h')) },
    { value: '7d', label: this.locale.translate(marker('platform.providers.last7d')) },
  ]);

  /** The name from the catalog, or the API's own word for a value this app has no name for. */
  protected nameOf(key: string | null, fallback: string): string {
    return key ? this.locale.translate(key) : fallback;
  }

  /** A duration in milliseconds written for the language; a dash when there is none. */
  protected milliseconds(ms: number | null): string {
    return ms === null
      ? this.locale.translate(marker('platform.common.none'))
      : this.locale.translate(marker('platform.providers.milliseconds'), { ms });
  }

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  ngOnInit(): void {
    this.store.load(this.store.window());
  }

  protected onWindowChange(value: string): void {
    if (value === '1h' || value === '24h' || value === '7d') {
      this.store.setWindow(value);
    }
  }

  protected healthVerdict(
    v: ProviderStatus['providers'][number]['verdict'],
  ): 'healthy' | 'degraded' | 'unhealthy' {
    return v === 'degraded' || v === 'unhealthy' ? v : 'healthy';
  }
}
