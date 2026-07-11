import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { ChannelBadgeComponent } from '../../../shared/components/channel-badge/channel-badge.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { SearchInputComponent } from '../../../shared/components/search-input/search-input.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { ToolbarComponent } from '../../../shared/components/toolbar/toolbar.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { CustomerFixture } from '../../../shared/fixtures/fixture.models';
import { PAGE_ROUTE, RoutedPageStore } from '../routed-page.store';

@Component({
  selector: 'app-customers',
  imports: [
    AvatarComponent,
    ChannelBadgeComponent,
    DataTableComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SearchInputComponent,
    StatusBadgeComponent,
    ToolbarComponent,
  ],
  providers: [RoutedPageStore, { provide: PAGE_ROUTE, useValue: 'customers' }],
  template: `
    <app-page-container>
      <app-page-header
        title="Customers"
        [description]="'Customer profiles and conversation history'"
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
      } @else if (customers().length) {
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
      } @else if (query()) {
        <app-empty-state
          icon="@tui.search-x"
          title="No customers match"
          description="Try adjusting the search or filters to find what you're looking for."
        >
          <button type="button" (click)="query.set('')">Clear search</button>
        </app-empty-state>
      } @else {
        <app-empty-state
          icon="@tui.users"
          title="No customers yet"
          description="Customer profiles and conversation history will appear here once your customers start interacting."
        >
        </app-empty-state>
      }
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
  protected readonly page = inject(RoutedPageStore);
  protected readonly hasError = computed(() => this.page.error() !== null);
  protected readonly query = signal('');
  protected readonly customersData = signal<readonly CustomerFixture[]>([]);

  protected readonly customers = computed(() => {
    const query = this.query().trim().toLowerCase();
    return query
      ? this.customersData().filter((customer) =>
          `${customer.name} ${customer.email}`.toLowerCase().includes(query),
        )
      : this.customersData();
  });

  constructor() {
    effect(() => {
      const data = this.page.data();
      if (data?.page === 'customers') {
        this.customersData.set(data.data);
      }
    });
  }

  protected retry(): void {
    this.page.retry();
  }
}
