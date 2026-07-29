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
  template: `
    <app-page-container>
      <app-page-header title="AI provider status" />

      @if (store.loading() && !store.status()) {
        <app-loading-state label="Loading provider status…" />
      } @else if (store.error(); as error) {
        <app-inline-alert tone="error">{{ error }}</app-inline-alert>
      }

      @if (store.status(); as status) {
        <div class="toolbar">
          <app-select-filter
            label="Window"
            [options]="windowOptions"
            [value]="store.window()"
            (valueChange)="onWindowChange($event)"
          />
          <span class="utc-note"
            >Window: {{ status.window.from }} – {{ status.window.to }} UTC</span
          >
        </div>

        <div class="cards">
          @for (prov of status.providers; track prov.provider) {
            <app-dashboard-card [title]="prov.provider">
              <div class="prov-row">
                @if (prov.verdict === 'no_activity' || prov.verdict === 'insufficient_data') {
                  <app-status-badge [status]="prov.verdict" tone="neutral" />
                } @else {
                  <app-health-indicator [verdict]="healthVerdict(prov.verdict)" />
                }
                <p class="meta">
                  Calls: {{ prov.totalCalls }} · Success: {{ prov.successes }} · Failures:
                  {{ prov.failures }}
                </p>
                <p class="meta">
                  Failure rate:
                  {{
                    prov.failureRate !== null
                      ? (prov.failureRate * 100 | number: '1.0-1') + '%'
                      : '—'
                  }}
                </p>
                <p class="meta">
                  p50: {{ prov.p50LatencyMs !== null ? prov.p50LatencyMs + 'ms' : '—' }} · p95:
                  {{ prov.p95LatencyMs !== null ? prov.p95LatencyMs + 'ms' : '—' }}
                </p>
                <p class="meta">Tokens in: {{ prov.inputTokens }} · out: {{ prov.outputTokens }}</p>
                <details class="categories">
                  <summary>Failures by category</summary>
                  <ul>
                    <li>Authentication: {{ prov.failuresByCategory.authentication }}</li>
                    <li>Rate limited: {{ prov.failuresByCategory.rateLimited }}</li>
                    <li>Unavailable: {{ prov.failuresByCategory.unavailable }}</li>
                    <li>Timeout: {{ prov.failuresByCategory.timeout }}</li>
                    <li>Invalid request: {{ prov.failuresByCategory.invalidRequest }}</li>
                  </ul>
                </details>
              </div>
            </app-dashboard-card>
          }
        </div>

        <section class="failures">
          <h3>Recent Failures</h3>
          @if (status.recentFailures.length === 0) {
            <app-empty-state
              icon="@tui.circle-check"
              title="No recent failures"
              description="No failed AI calls were recorded in this window."
            />
          } @else {
            <app-data-table>
              <table>
                <thead>
                  <tr>
                    <th>Provider</th>
                    <th>Model</th>
                    <th>Category</th>
                    <th>Latency</th>
                    <th>Time (UTC)</th>
                  </tr>
                </thead>
                <tbody>
                  @for (f of status.recentFailures; track $index) {
                    <tr>
                      <td>{{ f.provider }}</td>
                      <td>{{ f.model }}</td>
                      <td>{{ f.errorCategory ?? '—' }}</td>
                      <td>{{ f.latencyMs }}ms</td>
                      <td>{{ f.occurredAt }}</td>
                    </tr>
                  }
                </tbody>
              </table>
            </app-data-table>
          }
        </section>
      } @else {
        <app-empty-state
          icon="@tui.bot"
          title="No provider data available"
          description="Provider metrics will appear once AI calls are recorded."
        />
      }
    </app-page-container>
  `,
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
