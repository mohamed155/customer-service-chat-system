import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { PlatformBillingStore } from './platform-billing.store';

function formatCents(cents: number): string {
  return `$${(cents / 100).toFixed(2)}`;
}

@Component({
  selector: 'app-platform-billing',
  standalone: true,
  imports: [
    FormsModule,
    ButtonComponent,
    DataTableComponent,
    InlineAlertComponent,
    LoadingStateComponent,
    StatusBadgeComponent,
  ],
  providers: [PlatformBillingStore],
  template: `
    <div class="page">
      <h1 class="page-title">Platform Billing</h1>

      @if (store.loading()) {
        <app-loading-state label="Loading billing data…" />
      } @else if (store.error(); as error) {
        <app-inline-alert tone="error">{{ error }}</app-inline-alert>
      } @else {
        <section class="section">
          <div class="section-header">
            <h2>Plan Catalog</h2>
            @if (canManage()) {
              <div class="form-inline">
                <input type="text" placeholder="Name" [(ngModel)]="newPlanName" class="input-sm" />
                <input
                  type="number"
                  placeholder="Price (cents)"
                  [(ngModel)]="newPlanPrice"
                  class="input-sm"
                />
                <input
                  type="number"
                  placeholder="Token allowance"
                  [(ngModel)]="newPlanTokens"
                  class="input-sm"
                />
                <input
                  type="number"
                  placeholder="Conv. allowance"
                  [(ngModel)]="newPlanConversations"
                  class="input-sm"
                />
                <app-button
                  variant="primary"
                  size="sm"
                  (pressed)="onCreatePlan()"
                  [disabled]="!newPlanName() || !newPlanPrice() || store.saving()"
                >
                  {{ store.saving() ? 'Saving…' : 'Create' }}
                </app-button>
              </div>
            }
          </div>

          @if (store.plans().length === 0) {
            <p class="empty-muted">No plans defined yet.</p>
          } @else {
            <app-data-table>
              <table>
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Price</th>
                    <th>Tokens</th>
                    <th>Conversations</th>
                    <th>Default</th>
                    <th>Status</th>
                    @if (canManage()) {
                      <th>Actions</th>
                    }
                  </tr>
                </thead>
                <tbody>
                  @for (plan of store.plans(); track plan.id) {
                    <tr>
                      <td>{{ plan.name }}</td>
                      <td>{{ formatCents(plan.monthlyPriceCents) }}/mo</td>
                      <td>{{ plan.tokenAllowance ?? 'Unlimited' }}</td>
                      <td>{{ plan.conversationAllowance ?? 'Unlimited' }}</td>
                      <td>
                        @if (plan.isDefault) {
                          <app-status-badge status="Default" tone="accent" />
                        }
                      </td>
                      <td>
                        <app-status-badge [status]="plan.status" tone="neutral" />
                      </td>
                      @if (canManage()) {
                        <td>
                          @if (plan.status === 'active') {
                            <app-button
                              variant="danger"
                              size="sm"
                              (pressed)="store.retirePlan(plan.id)"
                              [disabled]="store.saving()"
                            >
                              Retire
                            </app-button>
                          } @else if (plan.status === 'retired') {
                            <app-button
                              variant="primary"
                              size="sm"
                              (pressed)="store.activatePlan(plan.id)"
                              [disabled]="store.saving()"
                            >
                              Activate
                            </app-button>
                          }
                        </td>
                      }
                    </tr>
                  }
                </tbody>
              </table>
            </app-data-table>
          }
        </section>

        <section class="section">
          <div class="section-header">
            <h2>Tenant Subscriptions</h2>
          </div>

          @if (store.subscriptions().length === 0) {
            <p class="empty-muted">No subscriptions found.</p>
          } @else {
            <app-data-table>
              <table>
                <thead>
                  <tr>
                    <th>Tenant</th>
                    <th>Plan</th>
                    <th>Status</th>
                    <th>Explicit</th>
                    @if (canManage()) {
                      <th>Assign</th>
                    }
                  </tr>
                </thead>
                <tbody>
                  @for (sub of store.subscriptions(); track sub.tenantId) {
                    <tr>
                      <td>{{ sub.tenantName }}</td>
                      <td>{{ sub.planName }}</td>
                      <td>
                        <app-status-badge [status]="sub.status" tone="neutral" />
                      </td>
                      <td>{{ sub.explicit ? 'Yes' : 'No' }}</td>
                      @if (canManage()) {
                        <td>
                          <select
                            class="input-sm"
                            [value]="sub.planId"
                            (change)="onAssignPlan(sub.tenantId, $event)"
                          >
                            @for (plan of store.plans(); track plan.id) {
                              <option [value]="plan.id">{{ plan.name }}</option>
                            }
                          </select>
                        </td>
                      }
                    </tr>
                  }
                </tbody>
              </table>
            </app-data-table>
          }
        </section>
      }
    </div>
  `,
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
  protected readonly formatCents = formatCents;

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
    this.store.assignSubscription(tenantId, planId);
  }
}
