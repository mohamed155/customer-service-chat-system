import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { HealthIndicatorComponent } from '../../../shared/components/health-indicator/health-indicator.component';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { SystemHealthStore } from './system-health.store';

@Component({
  selector: 'app-system-health',
  standalone: true,
  imports: [
    HealthIndicatorComponent,
    ButtonComponent,
    DashboardCardComponent,
    EmptyStateComponent,
    InlineAlertComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
  ],
  providers: [SystemHealthStore],
  template: `
    <app-page-container>
      <app-page-header title="System health" />

      <div class="toolbar">
        <app-button variant="primary" size="sm" (pressed)="store.load()">Refresh</app-button>
        @if (store.health(); as health) {
          <span class="utc-note">Read at {{ health.readAt }} UTC</span>
        }
      </div>

      @if (store.loading() && !store.health()) {
        <app-loading-state label="Loading system health…" />
      } @else if (store.error(); as error) {
        <app-inline-alert tone="error">{{ error }}</app-inline-alert>
      }

      @if (store.health(); as health) {
        <div class="overall">
          <app-health-indicator [verdict]="health.overall" />
        </div>

        <div class="cards">
          @for (dep of health.dependencies; track dep.name) {
            <app-dashboard-card [title]="dep.name">
              <div class="dep-row">
                <app-health-indicator [verdict]="dep.verdict" />
                @if (dep.reason) {
                  <p class="reason">{{ dep.reason }}</p>
                }
                <p class="meta">Probe: {{ dep.probeMs }}ms</p>
                @if (dep.detail?.kind === 'queue') {
                  <p class="meta">
                    Pending: {{ dep.detail.pendingCount }} · Stuck: {{ dep.detail.stuckRetryCount }}
                  </p>
                  @if (dep.detail.oldestPendingAgeSeconds !== null) {
                    <p class="meta">Oldest: {{ dep.detail.oldestPendingAgeSeconds }}s</p>
                  }
                }
                @if (dep.detail?.kind === 'storage') {
                  <p class="meta">
                    Backend: {{ dep.detail.backend }} · S3 configured: {{ dep.detail.s3Configured }}
                  </p>
                }
              </div>
            </app-dashboard-card>
          }
        </div>
      } @else {
        <app-empty-state
          icon="@tui.alert-circle"
          title="No health data available"
          description="Health readings will appear once the first probe completes."
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
      .overall {
        margin-bottom: var(--app-space-4);
      }
      .cards {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
        gap: var(--app-space-3);
      }
      .dep-row {
        display: flex;
        flex-direction: column;
        gap: var(--app-space-1);
      }
      .reason {
        margin: 0;
        font-size: var(--app-font-sm);
        color: var(--app-red);
      }
      .meta {
        margin: 0;
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SystemHealthComponent {
  protected readonly store = inject(SystemHealthStore);
}
