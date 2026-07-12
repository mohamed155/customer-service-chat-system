import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { HasPermissionDirective } from '../../../core/authz/has-permission.directive';
import { APP_PATHS } from '../../../core/router/app-paths';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { DataTableComponent } from '../../../shared/components/data-table/data-table.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { SearchInputComponent } from '../../../shared/components/search-input/search-input.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { TenantStatusFilter, TenantsStore } from './tenants.store';

@Component({
  selector: 'app-tenant-list',
  imports: [
    RouterLink,
    PageContainerComponent,
    PageHeaderComponent,
    DataTableComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    SearchInputComponent,
    StatusBadgeComponent,
    HasPermissionDirective,
  ],
  template: `
    <app-page-container>
      <app-page-header title="Tenants" description="Manage customer organizations">
        <a
          *appHasPermission="'platform.tenants.manage'"
          class="new-link"
          [routerLink]="[
            '/',
            APP_PATHS.platform.base,
            APP_PATHS.platform.tenants,
            APP_PATHS.platform.newTenant,
          ]"
        >
          New tenant
        </a>
      </app-page-header>

      <div class="toolbar">
        <app-search-input
          placeholder="Search by name or slug"
          [value]="searchInput()"
          (valueChange)="onSearchChange($event)"
        />
        <select
          class="status-filter"
          [value]="statusFilter() ?? ''"
          (change)="onStatusFilterChange($event)"
          aria-label="Status filter"
        >
          <option value="">All statuses</option>
          <option value="active">Active</option>
          <option value="suspended">Suspended</option>
        </select>
      </div>

      @if (store.loading()) {
        <app-loading-state />
      } @else if (store.status() === 'error') {
        <app-empty-state
          icon="@tui.alert-circle"
          title="Something went wrong"
          description="We couldn't load the tenants. Please try again."
        >
          <button type="button" class="primary-button" (click)="retry()">Try again</button>
        </app-empty-state>
      } @else if (store.items().length === 0 && (searchInput() || statusFilter())) {
        <app-empty-state
          icon="@tui.search-x"
          title="No tenants match"
          description="Try adjusting your search or filters."
        >
          <button type="button" class="primary-button" (click)="clearFilters()">
            Clear filters
          </button>
        </app-empty-state>
      } @else if (store.items().length === 0) {
        <app-empty-state
          icon="@tui.users"
          title="No tenants yet"
          description="Customer organizations will appear here once they are created."
        >
          <a
            *appHasPermission="'platform.tenants.manage'"
            class="primary-button"
            [routerLink]="[
              '/',
              APP_PATHS.platform.base,
              APP_PATHS.platform.tenants,
              APP_PATHS.platform.newTenant,
            ]"
          >
            New tenant
          </a>
        </app-empty-state>
      } @else {
        <app-data-table>
          <table>
            <thead>
              <tr>
                <th>Name</th>
                <th>Slug</th>
                <th>Status</th>
                <th>Plan</th>
              </tr>
            </thead>
            <tbody>
              @for (tenant of store.items(); track tenant.id) {
                <tr>
                  <td>
                    <a
                      class="name-link"
                      [routerLink]="[
                        '/',
                        APP_PATHS.platform.base,
                        APP_PATHS.platform.tenants,
                        tenant.id,
                      ]"
                    >
                      {{ tenant.name }}
                    </a>
                  </td>
                  <td class="muted">{{ tenant.slug }}</td>
                  <td>
                    <app-status-badge
                      [status]="tenant.status"
                      [tone]="tenant.status === 'active' ? 'green' : 'neutral'"
                    />
                  </td>
                  <td class="muted">{{ tenant.plan }}</td>
                </tr>
              }
            </tbody>
          </table>
        </app-data-table>
        @if (store.hasMore()) {
          <div class="load-more">
            @if (store.loadMoreError(); as err) {
              <div class="load-more-error" role="alert">{{ err.message }}</div>
            }
            <button
              type="button"
              class="load-more-button"
              (click)="store.loadMore()"
              [disabled]="store.loadingMore()"
            >
              {{ store.loadingMore() ? 'Loading…' : 'Load more' }}
            </button>
          </div>
        }
      }
    </app-page-container>
  `,
  styles: [
    `
      .toolbar {
        display: flex;
        gap: var(--app-space-3);
        margin-bottom: var(--app-space-4);
        align-items: center;
      }
      .toolbar app-search-input {
        flex: 1;
        min-width: 0;
      }
      .status-filter {
        height: 38px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
        font: inherit;
      }
      .status-filter:focus {
        outline: 0;
        border-color: var(--app-accent);
        box-shadow: 0 0 0 3px var(--app-accent-soft);
      }
      .new-link {
        height: 38px;
        display: inline-flex;
        align-items: center;
        padding: 0 var(--app-space-3);
        border: 0;
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-on, white);
        text-decoration: none;
        font-weight: 600;
        font-size: var(--app-font-sm);
      }
      .new-link:hover {
        opacity: 0.92;
      }
      .new-link:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      .primary-button {
        height: 38px;
        padding: 0 var(--app-space-4);
        border: 0;
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-on, white);
        font: inherit;
        font-weight: 600;
        cursor: pointer;
      }
      .primary-button:hover {
        opacity: 0.92;
      }
      .primary-button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      .muted {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .name-link {
        color: var(--app-text);
        text-decoration: none;
        font-weight: 500;
      }
      .name-link:hover {
        color: var(--app-accent);
        text-decoration: underline;
      }
      .name-link:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-radius: var(--app-radius-xs);
      }
      .load-more {
        display: flex;
        justify-content: center;
        margin-top: var(--app-space-4);
      }
      .load-more-button {
        height: 38px;
        padding: 0 var(--app-space-4);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font: inherit;
        cursor: pointer;
      }
      .load-more-button:hover {
        background: var(--app-panel-2);
        border-color: var(--app-border-strong);
      }
      .load-more-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
      .load-more-button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      .load-more-error {
        margin-bottom: var(--app-space-2);
        padding: var(--app-space-2) var(--app-space-3);
        background: var(--app-red-bg, #fef3f2);
        border: 1px solid var(--app-red, #d92d20);
        border-radius: var(--app-radius-md);
        color: var(--app-red, #d92d20);
        font-size: var(--app-font-sm);
        font-weight: 500;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TenantListComponent implements OnInit {
  protected readonly store = inject(TenantsStore);
  protected readonly APP_PATHS = APP_PATHS;

  protected readonly searchInput = signal('');
  protected readonly statusFilter = signal<TenantStatusFilter>(null);

  ngOnInit(): void {
    this.store.load();
  }

  protected onSearchChange(value: string): void {
    this.searchInput.set(value);
    this.store.setQueryInput(value);
  }

  protected onStatusFilterChange(event: Event): void {
    const value = (event.target as HTMLSelectElement).value;
    const filter: TenantStatusFilter = value === '' ? null : (value as 'active' | 'suspended');
    this.statusFilter.set(filter);
    this.store.setStatusFilter(filter);
  }

  protected clearFilters(): void {
    this.searchInput.set('');
    this.statusFilter.set(null);
    this.store.resetFilters();
  }

  protected retry(): void {
    this.store.load();
  }
}
