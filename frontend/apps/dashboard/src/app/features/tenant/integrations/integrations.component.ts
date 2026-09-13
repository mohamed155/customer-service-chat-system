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
  templateUrl: './integrations.component.html',
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
