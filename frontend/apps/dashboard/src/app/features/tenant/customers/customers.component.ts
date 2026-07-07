import { ChangeDetectionStrategy, Component, computed, signal } from '@angular/core';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { ChannelBadgeComponent } from '../../../shared/components/channel-badge/channel-badge.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { SearchInputComponent } from '../../../shared/components/search-input/search-input.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { ToolbarComponent } from '../../../shared/components/toolbar/toolbar.component';
import { CUSTOMER_FIXTURES } from '../../../shared/fixtures/customer.fixtures';

@Component({
  selector: 'app-customers',
  imports: [
    AvatarComponent,
    ChannelBadgeComponent,
    DataTableComponent,
    PageContainerComponent,
    SearchInputComponent,
    StatusBadgeComponent,
    ToolbarComponent,
  ],
  template: `
    <app-page-container>
      <div class="stack">
        <app-toolbar>
          <app-search-input toolbar-start placeholder="Search customers" [(value)]="query" />
          <select toolbar-end aria-label="Tier filter">
            <option>All tiers</option>
            <option>Enterprise</option>
            <option>Pro</option>
            <option>Free</option>
          </select>
        </app-toolbar>
        <app-data-table>
          <table>
            <thead>
              <tr>
                <th>Customer</th>
                <th>Channel</th>
                <th>Tier</th>
                <th>Last interaction</th>
                <th>Interactions</th>
                <th>CSAT</th>
                <th>Spend / Orders</th>
              </tr>
            </thead>
            <tbody>
              @for (customer of customers(); track customer.id) {
                <tr>
                  <td>
                    <div class="customer">
                      <app-avatar [initials]="customer.avatarInitials" size="sm" />
                      <span
                        ><strong>{{ customer.name }}</strong
                        ><small>{{ customer.email }}</small></span
                      >
                    </div>
                  </td>
                  <td><app-channel-badge [channel]="customer.channel" /></td>
                  <td><app-status-badge [status]="customer.tier" tone="accent" /></td>
                  <td class="muted">{{ customer.lastInteractionAt }}</td>
                  <td>{{ customer.interactions }}</td>
                  <td>{{ customer.csat }}%</td>
                  <td>
                    {{ customer.totalSpend }} <span class="muted">/ {{ customer.orders }}</span>
                  </td>
                </tr>
              }
            </tbody>
          </table>
        </app-data-table>
      </div>
    </app-page-container>
  `,
  styles: [
    `
      .stack {
        display: grid;
        gap: var(--app-space-4);
      }
      select {
        height: 38px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
      }
      .customer {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
      }
      strong,
      small {
        display: block;
      }
      strong {
        color: var(--app-text);
      }
      small {
        color: var(--app-text-3);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CustomersComponent {
  protected readonly query = signal('');
  protected readonly customers = computed(() => {
    const query = this.query().trim().toLowerCase();
    return query
      ? CUSTOMER_FIXTURES.filter((customer) =>
          `${customer.name} ${customer.email}`.toLowerCase().includes(query),
        )
      : CUSTOMER_FIXTURES;
  });
}
