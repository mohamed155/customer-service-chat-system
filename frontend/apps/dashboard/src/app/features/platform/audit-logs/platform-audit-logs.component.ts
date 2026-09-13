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
  templateUrl: './platform-audit-logs.component.html',
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
