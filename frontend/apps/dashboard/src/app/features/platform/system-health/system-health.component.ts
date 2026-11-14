import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { ApiError } from '../../../core/api/api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { LocaleDatePipe } from '../../../core/i18n/format';
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
    LocaleDatePipe,
    TranslocoPipe,
  ],
  providers: [SystemHealthStore, provideTranslocoScope('platform')],
  templateUrl: './system-health.component.html',
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

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }
}
