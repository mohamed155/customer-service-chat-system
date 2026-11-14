import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { ApiError } from '../../../core/api/api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { FormsModule } from '@angular/forms';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { SearchInputComponent } from '../../../shared/components/search-input/search-input.component';
import { SelectFilterComponent } from '../../../shared/components/select-filter/select-filter.component';
import { ToolbarComponent } from '../../../shared/components/toolbar/toolbar.component';
import { AuditDetailDrawerComponent } from '../../../shared/components/audit-detail-drawer/audit-detail-drawer.component';
import { AuditLogTableComponent } from '../../../shared/components/audit-log-table/audit-log-table.component';
import { PlatformAuditLogsStore } from './platform-audit-logs.store';

/** The audit categories the API takes, each with the catalog key of its name. */
const CATEGORIES: readonly { value: string; key: string }[] = [
  { value: 'all', key: marker('platform.audit.categories.all') },
  { value: 'auth', key: marker('platform.audit.categories.auth') },
  { value: 'tenant', key: marker('platform.audit.categories.tenant') },
  { value: 'members', key: marker('platform.audit.categories.members') },
  { value: 'prompts', key: marker('platform.audit.categories.prompts') },
  { value: 'ai', key: marker('platform.audit.categories.ai') },
  { value: 'tools', key: marker('platform.audit.categories.tools') },
  { value: 'billing', key: marker('platform.audit.categories.billing') },
  { value: 'conversations', key: marker('platform.audit.categories.conversations') },
  { value: 'customers', key: marker('platform.audit.categories.customers') },
  { value: 'escalations', key: marker('platform.audit.categories.escalations') },
  { value: 'knowledge', key: marker('platform.audit.categories.knowledge') },
  { value: 'widgets', key: marker('platform.audit.categories.widgets') },
];

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
    TranslocoPipe,
  ],
  providers: [PlatformAuditLogsStore, provideTranslocoScope('platform')],
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
  private readonly locale = inject(LocaleService);

  /** The category filter: the value the API takes and its name in the active language. */
  protected readonly categoryOptions = computed(() =>
    CATEGORIES.map((c) => ({ value: c.value, label: this.locale.translate(c.key) })),
  );

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  protected onFromChange(value: string): void {
    this.store.setDateRange(value, this.store.to());
  }

  protected onToChange(value: string): void {
    this.store.setDateRange(this.store.from(), value);
  }
}
