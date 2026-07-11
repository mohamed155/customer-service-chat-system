import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PAGE_ROUTE, RoutedPageStore } from '../routed-page.store';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { MetricCardComponent } from '../../../shared/components/metric-card/metric-card.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { SparklineComponent } from '../../../shared/components/sparkline/sparkline.component';
import { ToolbarComponent } from '../../../shared/components/toolbar/toolbar.component';

@Component({
  selector: 'app-analytics',
  imports: [
    DashboardCardComponent,
    DataTableComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    MetricCardComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SectionHeaderComponent,
    SparklineComponent,
    ToolbarComponent,
  ],
  providers: [RoutedPageStore, { provide: PAGE_ROUTE, useValue: 'analytics' }],
  template: `
    <app-page-container>
      <app-page-header title="Analytics" [description]="'Trends across every channel'" />
      @if (page.loading()) {
        <app-loading-state />
      } @else if (hasError()) {
        <app-empty-state
          icon="@tui.alert-circle"
          title="Something went wrong"
          description="We couldn't load this page. Please try again."
        >
          <button type="button" (click)="retry()">Try again</button>
        </app-empty-state>
      } @else if (hasData()) {
        <div class="stack">
          <app-toolbar>
            <strong toolbar-start>Performance dashboard</strong>
            <select
              toolbar-end
              aria-label="Date range"
              [value]="dateRange()"
              (change)="setDateRange($event)"
            >
              <option>Last 7 days</option>
              <option>Last 30 days</option>
              <option>This quarter</option>
            </select>
            <select
              toolbar-end
              aria-label="Channel"
              [value]="channel()"
              (change)="setChannel($event)"
            >
              <option>All channels</option>
              <option>Website</option>
              <option>WhatsApp</option>
              <option>Telegram</option>
            </select>
          </app-toolbar>

          <section class="metrics">
            @for (metric of pageData()?.metrics; track metric.id) {
              <app-metric-card [metric]="metric" />
            }
          </section>

          <section class="charts">
            @for (chart of pageData()?.charts; track chart.id) {
              <app-dashboard-card>
                <app-section-header
                  card-header
                  [title]="chart.label"
                  subtitle="Fixture trend series"
                />
                <app-sparkline [points]="chart.points" [colorToken]="chart.colorToken" />
              </app-dashboard-card>
            }
          </section>

          <app-data-table>
            <table>
              <thead>
                <tr>
                  <th>Article</th>
                  <th>Category</th>
                  <th>Uses</th>
                  <th>Resolution rate</th>
                </tr>
              </thead>
              <tbody>
                @for (article of pageData()?.topArticles; track article.id) {
                  <tr>
                    <td>{{ article.title }}</td>
                    <td class="muted">{{ article.category }}</td>
                    <td>{{ article.uses }}</td>
                    <td>{{ article.resolutionRate }}%</td>
                  </tr>
                }
              </tbody>
            </table>
          </app-data-table>
        </div>
      } @else {
        <app-empty-state
          icon="@tui.chart-line"
          title="No analytics data"
          description="Reports and charts will populate as customer interactions are recorded."
        />
      }
    </app-page-container>
  `,
  styles: [
    `
      .stack {
        display: grid;
        gap: var(--app-space-4);
      }
      strong {
        color: var(--app-text);
      }
      select {
        height: 38px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
      }
      .metrics {
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: var(--app-space-4);
      }
      .charts {
        display: grid;
        grid-template-columns: repeat(4, minmax(0, 1fr));
        gap: var(--app-space-4);
      }
      @media (max-width: 1200px) {
        .metrics,
        .charts {
          grid-template-columns: repeat(2, minmax(0, 1fr));
        }
      }
      @media (max-width: 768px) {
        .metrics,
        .charts {
          grid-template-columns: 1fr;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AnalyticsComponent {
  protected readonly page = inject(RoutedPageStore);
  protected readonly hasData = computed(() => this.page.data() !== undefined);
  protected readonly hasError = computed(() => this.page.error() !== null);
  protected readonly dateRange = signal('Last 30 days');
  protected readonly channel = signal('All channels');

  protected readonly pageData = computed(() => {
    const data = this.page.data();
    if (data?.page === 'analytics') return data.data;
    return undefined;
  });

  protected setDateRange(event: Event): void {
    this.dateRange.set((event.target as HTMLSelectElement).value);
  }

  protected setChannel(event: Event): void {
    this.channel.set((event.target as HTMLSelectElement).value);
  }

  protected retry(): void {
    this.page.retry();
  }
}
