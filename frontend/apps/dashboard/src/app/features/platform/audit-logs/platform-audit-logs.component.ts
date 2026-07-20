import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { SearchInputComponent } from '../../../shared/components/search-input/search-input.component';
import { SelectFilterComponent } from '../../../shared/components/select-filter/select-filter.component';
import { ToolbarComponent } from '../../../shared/components/toolbar/toolbar.component';
import { AuditDetailDrawerComponent } from '../../../shared/components/audit-detail-drawer/audit-detail-drawer.component';
import { AuditLogTableComponent } from '../../../shared/components/audit-log-table/audit-log-table.component';
import { PlatformAuditLogsStore } from './platform-audit-logs.store';

@Component({
  selector: 'app-platform-audit-logs',
  standalone: true,
  imports: [
    FormsModule,
    ButtonComponent,
    InlineAlertComponent,
    SearchInputComponent,
    SelectFilterComponent,
    ToolbarComponent,
    AuditDetailDrawerComponent,
    AuditLogTableComponent,
  ],
  providers: [PlatformAuditLogsStore],
  template: `
    <div class="page">
      <h1>Platform Audit Logs</h1>

      <app-toolbar>
        <app-select-filter
          label="Category"
          [options]="categoryOptions"
          [value]="store.category() ?? 'all'"
          (valueChange)="store.setCategory($event)"
        />
        <input type="date" [ngModel]="store.from()" (ngModelChange)="onFromChange($event)" />
        <input type="date" [ngModel]="store.to()" (ngModelChange)="onToChange($event)" />
        <app-search-input
          placeholder="Actor ID…"
          [value]="store.actorId() ?? ''"
          (searchSubmit)="store.setActor($event || null)"
        />
        <app-search-input
          placeholder="Tenant ID…"
          [value]="store.tenantId() ?? ''"
          (searchSubmit)="store.setTenant($event || null)"
        />
      </app-toolbar>

      @if (store.error(); as error) {
        <app-inline-alert tone="error">{{ error }}</app-inline-alert>
      }

      <app-audit-log-table
        [entries]="store.entries()"
        [loading]="store.loading()"
        [showTenantColumn]="true"
        (rowSelected)="store.openEntry($event)"
      />

      @if (store.hasMore()) {
        <div class="load-more">
          <app-button
            variant="secondary"
            size="sm"
            (pressed)="store.loadMore()"
            [disabled]="store.loadingMore()"
          >
            {{ store.loadingMore() ? 'Loading…' : 'Load more' }}
          </app-button>
        </div>
      }

      <app-audit-detail-drawer
        [entry]="store.selectedEntry()"
        [open]="store.drawerOpen()"
        (closed)="store.closeDrawer()"
      />
    </div>
  `,
  styles: [
    `
      .page {
        padding: var(--app-space-4);
      }
      .load-more {
        display: flex;
        justify-content: center;
        padding: var(--app-space-3);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PlatformAuditLogsComponent {
  protected readonly store = inject(PlatformAuditLogsStore);
  protected readonly categoryOptions = [
    { value: 'all', label: 'All Categories' },
    { value: 'auth', label: 'Auth' },
    { value: 'tenant', label: 'Tenant' },
    { value: 'members', label: 'Members' },
    { value: 'prompts', label: 'Prompts' },
    { value: 'ai', label: 'AI' },
    { value: 'tools', label: 'Tools' },
    { value: 'billing', label: 'Billing' },
    { value: 'conversations', label: 'Conversations' },
    { value: 'customers', label: 'Customers' },
    { value: 'escalations', label: 'Escalations' },
    { value: 'knowledge', label: 'Knowledge' },
    { value: 'widgets', label: 'Widgets' },
  ];

  protected onFromChange(value: string): void {
    this.store.setDateRange(value, this.store.to());
  }

  protected onToChange(value: string): void {
    this.store.setDateRange(this.store.from(), value);
  }
}
