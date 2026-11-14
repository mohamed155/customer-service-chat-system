import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { TuiTextfield } from '@taiga-ui/core';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { ApiError } from '../../../core/api/api.models';
import { BILLING_PRICE_FORMAT } from '../../../core/billing/billing-currency';
import { errorMessageKey } from '../../../core/errors/error-message';
import { LocaleNumberPipe } from '../../../core/i18n/format';
import { LocaleService } from '../../../core/i18n/locale.service';
import { planStatusKey, subscriptionKey } from '../../../core/i18n/platform-labels';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { PlatformBillingStore } from './platform-billing.store';

@Component({
  selector: 'app-platform-billing',
  standalone: true,
  imports: [
    FormsModule,
    TuiTextfield,
    ButtonComponent,
    DataTableComponent,
    InlineAlertComponent,
    LoadingStateComponent,
    StatusBadgeComponent,
    LocaleNumberPipe,
    TranslocoPipe,
  ],
  providers: [PlatformBillingStore, provideTranslocoScope('platform')],
  templateUrl: './platform-billing.component.html',
  styles: [
    `
      .page {
        padding: var(--app-space-4);
      }
      .page-title {
        margin: 0 0 var(--app-space-5);
        font-size: var(--app-font-2xl);
        font-weight: 700;
        color: var(--app-text);
      }
      .section {
        margin-bottom: var(--app-space-6);
      }
      .section-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-3);
        flex-wrap: wrap;
        margin-bottom: var(--app-space-3);
      }
      .section-header h2 {
        margin: 0;
        font-size: var(--app-font-lg);
        font-weight: 650;
        color: var(--app-text);
      }
      .form-inline {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        flex-wrap: wrap;
      }
      .input-sm {
        height: 28px;
        padding: 0 var(--app-space-2);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font: inherit;
        font-size: var(--app-font-sm);
        box-sizing: border-box;
      }
      .input-sm:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-color: var(--app-accent);
      }
      .empty-muted {
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
        font-style: italic;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PlatformBillingComponent {
  protected readonly store = inject(PlatformBillingStore);
  protected readonly permissions = inject(PermissionsService);
  private readonly locale = inject(LocaleService);
  protected readonly priceFormat = BILLING_PRICE_FORMAT;
  protected readonly planStatusKey = planStatusKey;
  protected readonly subscriptionKey = subscriptionKey;

  /** The name from the catalog, or the API's own word for a value this app has no name for. */
  protected nameOf(key: string | null, fallback: string): string {
    return key ? this.locale.translate(key) : fallback;
  }

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  protected readonly newPlanName = signal('');
  protected readonly newPlanPrice = signal(0);
  protected readonly newPlanTokens = signal<number | null>(null);
  protected readonly newPlanConversations = signal<number | null>(null);

  protected readonly canManage = computed(() => this.permissions.has('platform.billing.manage'));

  protected onCreatePlan(): void {
    this.store.createPlan({
      name: this.newPlanName(),
      monthlyPriceCents: this.newPlanPrice(),
      tokenAllowance: this.newPlanTokens(),
      conversationAllowance: this.newPlanConversations(),
    });
    this.newPlanName.set('');
    this.newPlanPrice.set(0);
    this.newPlanTokens.set(null);
    this.newPlanConversations.set(null);
  }

  protected onAssignPlan(tenantId: string, event: Event): void {
    const planId = (event.target as HTMLSelectElement).value;
    this.store.assignSubscription(tenantId, planId, 'active');
  }
}
