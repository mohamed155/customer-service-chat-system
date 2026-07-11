import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PAGE_ROUTE, RoutedPageStore } from '../routed-page.store';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { IntegrationStatus } from '../../../shared/fixtures/fixture.models';

@Component({
  selector: 'app-integrations',
  imports: [
    DashboardCardComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
    TuiIcon,
  ],
  providers: [RoutedPageStore, { provide: PAGE_ROUTE, useValue: 'integrations' }],
  template: `
    <app-page-container>
      <app-page-header
        title="Integrations"
        [description]="'Connect channels and business systems'"
      />
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
        <app-section-header title="Integrations" subtitle="Connect channels and systems" />
        <section class="grid">
          @for (integration of integrations(); track integration.id) {
            <app-dashboard-card>
              <div class="head">
                <span class="icon"><tui-icon [icon]="integration.icon" /></span>
                <app-status-badge [status]="integration.status" [tone]="tone(integration.status)" />
              </div>
              <h2>{{ integration.name }}</h2>
              <p>{{ integration.description }}</p>
              <button type="button">{{ integration.actionLabel }}</button>
            </app-dashboard-card>
          }
        </section>
      } @else {
        <app-empty-state
          icon="@tui.plug"
          title="No integrations configured"
          description="Connect your channels and tools to extend the platform's capabilities."
        />
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
        gap: var(--app-space-3);
      }
      .icon {
        width: 38px;
        height: 38px;
        display: grid;
        place-items: center;
        border-radius: var(--app-radius-md);
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      h2 {
        margin: var(--app-space-4) 0 var(--app-space-2);
        color: var(--app-text);
        font-size: var(--app-font-lg);
      }
      p {
        min-height: 56px;
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        line-height: 1.5;
      }
      button {
        height: 36px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
        font-weight: 650;
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
  protected readonly page = inject(RoutedPageStore);
  protected readonly hasData = computed(() => this.page.data() !== undefined);
  protected readonly hasError = computed(() => this.page.error() !== null);

  protected readonly integrations = computed(() => {
    const data = this.page.data();
    if (data?.page === 'integrations') return data.data;
    return [];
  });

  protected tone(status: IntegrationStatus): 'green' | 'amber' | 'neutral' {
    return status === 'connected' ? 'green' : status === 'coming-soon' ? 'amber' : 'neutral';
  }

  protected retry(): void {
    this.page.retry();
  }
}
