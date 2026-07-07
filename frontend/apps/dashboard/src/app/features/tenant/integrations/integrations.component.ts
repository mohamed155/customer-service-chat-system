import { ChangeDetectionStrategy, Component } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { INTEGRATION_FIXTURES } from '../../../shared/fixtures/integration.fixtures';
import { IntegrationStatus } from '../../../shared/fixtures/fixture.models';

@Component({
  selector: 'app-integrations',
  imports: [
    DashboardCardComponent,
    PageContainerComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
    TuiIcon,
  ],
  template: `
    <app-page-container>
      <app-section-header title="Integrations" subtitle="Connect channels and systems" />
      <section class="grid">
        @for (integration of integrations; track integration.id) {
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
  protected readonly integrations = INTEGRATION_FIXTURES;

  protected tone(status: IntegrationStatus): 'green' | 'amber' | 'neutral' {
    return status === 'connected' ? 'green' : status === 'coming-soon' ? 'amber' : 'neutral';
  }
}
