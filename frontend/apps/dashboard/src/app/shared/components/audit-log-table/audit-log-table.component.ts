import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { DatePipe, SlicePipe } from '@angular/common';
import { AuditEntry } from '../../../core/api/tenant-api.models';
import { DataTableComponent } from '../data-table/data-table.component';
import { EmptyStateComponent } from '../empty-state/empty-state.component';
import { LoadingStateComponent } from '../loading-state/loading-state.component';

@Component({
  selector: 'app-audit-log-table',
  standalone: true,
  imports: [DatePipe, SlicePipe, DataTableComponent, EmptyStateComponent, LoadingStateComponent],
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
  readonly entries = input.required<AuditEntry[]>();
  readonly loading = input(false);
  readonly showTenantColumn = input(false);
  readonly rowSelected = output<AuditEntry>();
}
