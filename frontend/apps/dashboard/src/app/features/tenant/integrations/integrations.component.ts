import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { IntegrationListItem } from '../../../core/api/tenant-api.models';
import { APP_PATHS } from '../../../core/router/app-paths';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import {
  BadgeTone,
  StatusBadgeComponent,
} from '../../../shared/components/status-badge/status-badge.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { IntegrationsStore } from './integrations.store';

@Component({
  selector: 'app-integrations',
  imports: [
    DashboardCardComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    RouterLink,
    SectionHeaderComponent,
    StatusBadgeComponent,
  ],
  template: `
    <app-page-container>
      <app-page-header
        title="Integrations"
        [description]="'Connect channels and business systems'"
      />
      @if (loading() && items().length === 0) {
        <app-loading-state />
      } @else if (error()) {
        <app-empty-state
          icon="@tui.alert-circle"
          title="Something went wrong"
          [description]="error() ?? ''"
        >
          <button type="button" (click)="retry()">Try again</button>
        </app-empty-state>
      } @else if (items().length === 0) {
        <app-empty-state
          icon="@tui.plug"
          title="No integrations configured"
          description="Connect your channels and tools to extend the platform's capabilities."
        />
      } @else {
        <app-section-header title="Integrations" subtitle="Connect channels and systems" />
        <section class="grid">
          @for (integration of items(); track integration.slug) {
            <app-dashboard-card>
              <div class="head">
                <app-status-badge [status]="integration.status" [tone]="tone(integration.status)" />
                @if (!integration.isAvailable) {
                  <span class="badge-soon">Coming soon</span>
                }
              </div>
              <h2>{{ integration.name }}</h2>
              <p>{{ integration.description }}</p>
              <a class="action" [routerLink]="detailLink(integration.slug)">{{
                integration.status === 'connected' ? 'Configure' : 'View'
              }}</a>
            </app-dashboard-card>
          }
        </section>
      }
    </app-page-container>
  `,
  styles: [
    `
      .grid {
        display: grid;
        grid-template-columns: repeat(4, minmax(0, 1fr));
        gap: var(--app-space-4);
      }
      .head {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: var(--app-space-3);
        margin-bottom: var(--app-space-2);
      }
      .badge-soon {
        display: inline-block;
        padding: 2px var(--app-space-2);
        border-radius: var(--app-radius-sm);
        background: var(--app-panel-2);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        font-weight: 600;
      }
      h2 {
        margin: var(--app-space-2) 0 var(--app-space-2);
        color: var(--app-text);
        font-size: var(--app-font-lg);
      }
      p {
        min-height: 56px;
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        line-height: 1.5;
      }
      .action {
        display: inline-block;
        height: 36px;
        line-height: 34px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
        font-weight: 650;
        text-decoration: none;
      }
      @media (max-width: 1200px) {
        .grid {
          grid-template-columns: repeat(2, minmax(0, 1fr));
        }
      }
      @media (max-width: 768px) {
        .grid {
          grid-template-columns: 1fr;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class IntegrationsComponent {
  protected readonly store = inject(IntegrationsStore);
  protected readonly items = this.store.items;
  protected readonly loading = this.store.loading;
  protected readonly error = this.store.error;
  protected readonly hasLoaded = computed(() => this.items().length > 0);

  protected tone(status: IntegrationListItem['status']): BadgeTone {
    switch (status) {
      case 'connected':
        return 'green';
      case 'error':
        return 'red';
      case 'disconnected':
        return 'amber';
      default:
        return 'neutral';
    }
  }

  protected detailLink(slug: string): string[] {
    return ['/', APP_PATHS.tenant.base, APP_PATHS.tenant.integrationDetail.replace(':slug', slug)];
  }

  protected retry(): void {
    this.store.load();
  }
}
