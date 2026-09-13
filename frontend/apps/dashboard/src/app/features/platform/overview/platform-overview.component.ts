import { ChangeDetectionStrategy, Component, effect, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { TuiTextfield } from '@taiga-ui/core';
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
  ],
  providers: [PlatformOverviewStore],
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

  protected metricCards(
    summary: NonNullable<ReturnType<typeof this.store.summary>>,
  ): { metric: MetricCardData; subText?: string }[] {
    return [
      {
        metric: {
          id: 'tenants',
          label: 'Total Tenants',
          value: String(summary.tenants.total),
          icon: '@tui.users',
        },
        subText: `${summary.tenants.active} active · ${summary.tenants.suspended} suspended · ${summary.tenants.createdInRange} created in range`,
      },
      {
        metric: {
          id: 'conversations',
          label: 'Active Conversations',
          value: String(summary.activeConversations),
          icon: '@tui.message-circle',
        },
      },
      {
        metric: {
          id: 'ai-calls',
          label: 'AI Calls',
          value: String(summary.aiUsage.totalCalls),
          icon: '@tui.bot',
        },
        subText: `${summary.aiUsage.successfulCalls} success · ${summary.aiUsage.failedCalls} failed`,
      },
      {
        metric: {
          id: 'input-tokens',
          label: 'Input Tokens',
          value: String(summary.aiUsage.inputTokens),
          icon: '@tui.arrow-down-to-line',
        },
      },
      {
        metric: {
          id: 'output-tokens',
          label: 'Output Tokens',
          value: String(summary.aiUsage.outputTokens),
          icon: '@tui.arrow-up-from-line',
        },
      },
    ];
  }
}
