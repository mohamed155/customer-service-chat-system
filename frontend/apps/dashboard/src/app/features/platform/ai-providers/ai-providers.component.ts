import { DecimalPipe } from '@angular/common';
import { ChangeDetectionStrategy, Component, inject, OnInit } from '@angular/core';
import { FormsModule } from '@angular/forms';
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
    DecimalPipe,
    EmptyStateComponent,
    InlineAlertComponent,
    LoadingStateComponent,
    SelectFilterComponent,
    PageContainerComponent,
    PageHeaderComponent,
  ],
  providers: [AiProvidersStore],
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
  protected readonly windowOptions: SelectFilterOption[] = [
    { value: '1h', label: 'Last hour' },
    { value: '24h', label: 'Last 24 hours' },
    { value: '7d', label: 'Last 7 days' },
  ];

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
