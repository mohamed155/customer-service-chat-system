import { ChangeDetectionStrategy, Component, inject, input, output } from '@angular/core';
import { auditActionName, auditCategoryName } from '../../../core/i18n/audit-labels';
import { LocaleService } from '../../../core/i18n/locale.service';
import { SlicePipe } from '@angular/common';
import { LocaleDatePipe } from '../../../core/i18n/format';
import { AuditEntry } from '../../../core/api/tenant-api.models';
import { DataTableComponent } from '../data-table/data-table.component';
import { EmptyStateComponent } from '../empty-state/empty-state.component';
import { LoadingStateComponent } from '../loading-state/loading-state.component';
import { TranslocoPipe } from '@jsverse/transloco';

@Component({
  selector: 'app-audit-log-table',
  standalone: true,
  imports: [
    LocaleDatePipe,
    SlicePipe,
    DataTableComponent,
    EmptyStateComponent,
    LoadingStateComponent,
    TranslocoPipe,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './audit-log-table.component.html',
  styles: [
    `
      .clickable-row {
        cursor: pointer;
      }
      .clickable-row:hover td {
        background: var(--app-panel-2);
      }
      .clickable-row:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: -3px;
      }
      .staff-badge {
        display: inline-block;
        margin-left: var(--app-space-2);
        font-size: var(--app-font-xs);
        padding: 1px var(--app-space-2);
        border-radius: var(--app-radius-sm);
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
        font-weight: 600;
      }
      .deleted-badge {
        display: inline-block;
        margin-left: var(--app-space-1);
        font-size: var(--app-font-xs);
        padding: 1px var(--app-space-2);
        border-radius: var(--app-radius-sm);
        background: var(--app-red-soft, #f8d7da);
        color: var(--app-red, #721c24);
        font-weight: 600;
      }
      .category-label {
        display: block;
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
      }
      .resource-id {
        display: block;
        font-family: monospace;
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
      }
    `,
  ],
})
export class AuditLogTableComponent {
  private readonly locale = inject(LocaleService);

  /** An action's name in the active language; an action this app has no name for shows its code. */
  protected actionName(action: string): string {
    return auditActionName(action, (key) => this.locale.translate(key));
  }

  protected categoryName(category: string): string {
    return auditCategoryName(category, (key) => this.locale.translate(key));
  }

  readonly entries = input.required<AuditEntry[]>();
  readonly loading = input(false);
  readonly showTenantColumn = input(false);
  readonly rowSelected = output<AuditEntry>();
}
