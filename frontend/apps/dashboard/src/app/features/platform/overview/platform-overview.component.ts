import { ChangeDetectionStrategy, Component, effect, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { TuiTextfield } from '@taiga-ui/core';
import { ApiError } from '../../../core/api/api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { LocaleDatePipe, formatNumber } from '../../../core/i18n/format';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { APP_PATHS } from '../../../core/router/app-paths';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import {
  MetricCardComponent,
  MetricCardData,
} from '../../../shared/components/metric-card/metric-card.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { PlatformOverviewStore } from './platform-overview.store';

@Component({
  selector: 'app-platform-overview',
  standalone: true,
  imports: [
    FormsModule,
    RouterLink,
    TuiTextfield,
    ButtonComponent,
    EmptyStateComponent,
    InlineAlertComponent,
    LoadingStateComponent,
    MetricCardComponent,
    PageContainerComponent,
    PageHeaderComponent,
    LocaleDatePipe,
    TranslocoPipe,
  ],
  providers: [PlatformOverviewStore, provideTranslocoScope('platform')],
  templateUrl: './platform-overview.component.html',
  styles: [
    `
      .controls {
        display: flex;
        gap: var(--app-space-2);
        align-items: center;
        margin-bottom: var(--app-space-4);
        flex-wrap: wrap;
      }
      .cards {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
        gap: var(--app-space-3);
        margin-bottom: var(--app-space-4);
      }
      .card-cell {
        display: flex;
        flex-direction: column;
        gap: var(--app-space-1);
      }
      .sub {
        margin: 0;
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
      }
      .utc-note {
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
        margin: var(--app-space-2) 0;
      }
      .link {
        font-size: var(--app-font-sm);
        color: var(--app-accent);
        text-decoration: none;
      }
      .link:hover {
        text-decoration: underline;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PlatformOverviewComponent {
  protected readonly store = inject(PlatformOverviewStore);
  protected readonly permissions = inject(PermissionsService);
  private readonly locale = inject(LocaleService);
  protected readonly APP_PATHS = APP_PATHS;

  protected readonly fromValue = signal('');
  protected readonly toValue = signal('');

  protected readonly canViewDiagnostics = () => this.permissions.has('platform.diagnostics.view');

  constructor() {
    effect(() => {
      const summary = this.store.summary();
      if (summary && !this.fromValue()) {
        this.fromValue.set(summary.range.from);
        this.toValue.set(summary.range.to);
      }
    });
  }

  protected isZeroState(summary: NonNullable<ReturnType<typeof this.store.summary>>): boolean {
    return (
      summary.tenants.total === 0 &&
      summary.activeConversations === 0 &&
      summary.aiUsage.totalCalls === 0
    );
  }

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  protected metricCards(
    summary: NonNullable<ReturnType<typeof this.store.summary>>,
  ): { metric: MetricCardData; subText?: string }[] {
    const text = (key: string, params?: Record<string, unknown>) =>
      this.locale.translate(key, params);
    const number = (value: number) => formatNumber(value, this.locale.formatLocale());
    return [
      {
        metric: {
          id: 'tenants',
          label: text(marker('platform.overview.tenants')),
          value: number(summary.tenants.total),
          icon: '@tui.users',
        },
        subText: text(marker('platform.overview.tenantsSub'), {
          active: summary.tenants.active,
          suspended: summary.tenants.suspended,
          created: summary.tenants.createdInRange,
        }),
      },
      {
        metric: {
          id: 'conversations',
          label: text(marker('platform.overview.conversations')),
          value: number(summary.activeConversations),
          icon: '@tui.message-circle',
        },
      },
      {
        metric: {
          id: 'ai-calls',
          label: text(marker('platform.overview.aiCalls')),
          value: number(summary.aiUsage.totalCalls),
          icon: '@tui.bot',
        },
        subText: text(marker('platform.overview.aiCallsSub'), {
          success: summary.aiUsage.successfulCalls,
          failed: summary.aiUsage.failedCalls,
        }),
      },
      {
        metric: {
          id: 'input-tokens',
          label: text(marker('platform.overview.inputTokens')),
          value: number(summary.aiUsage.inputTokens),
          icon: '@tui.arrow-down-to-line',
        },
      },
      {
        metric: {
          id: 'output-tokens',
          label: text(marker('platform.overview.outputTokens')),
          value: number(summary.aiUsage.outputTokens),
          icon: '@tui.arrow-up-from-line',
        },
      },
    ];
  }
}
