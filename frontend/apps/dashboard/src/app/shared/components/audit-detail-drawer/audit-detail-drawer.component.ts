import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { DatePipe, JsonPipe } from '@angular/common';
import { AuditEntry } from '../../../core/api/tenant-api.models';
import { DialogShellComponent } from '../dialog-shell/dialog-shell.component';

@Component({
  selector: 'app-audit-detail-drawer',
  standalone: true,
  imports: [DatePipe, JsonPipe, DialogShellComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './audit-detail-drawer.component.html',
  styles: [
    `
      .drawer-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-bottom: var(--app-space-4);
      }
      .drawer-title {
        margin: 0;
        font-size: var(--app-font-lg);
        font-weight: 650;
      }
      .close-btn {
        background: none;
        border: none;
        font-size: 1.5rem;
        cursor: pointer;
        color: var(--app-text-2);
        padding: 0;
        line-height: 1;
      }
      .close-btn:hover {
        color: var(--app-text);
      }
      .def-list {
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: var(--app-space-3);
      }
      .def-row {
        display: flex;
        flex-direction: column;
        gap: var(--app-space-1);
      }
      .def-row dt {
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
        font-weight: 650;
        text-transform: uppercase;
      }
      .def-row dd {
        margin: 0;
        font-size: var(--app-font-sm);
      }
      .def-row code {
        font-size: var(--app-font-xs);
        padding: 1px var(--app-space-1);
        background: var(--app-bg);
        border-radius: var(--app-radius-sm);
      }
      .badge {
        display: inline-block;
        margin-left: var(--app-space-1);
        font-size: var(--app-font-xs);
        padding: 1px var(--app-space-2);
        border-radius: var(--app-radius-sm);
        font-weight: 600;
      }
      .badge-staff {
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      .badge-deleted {
        background: var(--app-red-soft, #f8d7da);
        color: var(--app-red, #721c24);
      }
      .meta-heading {
        margin: var(--app-space-4) 0 var(--app-space-2);
        font-size: var(--app-font-sm);
        font-weight: 650;
      }
      .meta-json {
        margin: 0;
        padding: var(--app-space-3);
        background: var(--app-bg);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        font-family: monospace;
        font-size: var(--app-font-xs);
        line-height: 1.6;
        white-space: pre-wrap;
        overflow-x: auto;
        max-height: 40vh;
        overflow-y: auto;
      }
    `,
  ],
})
export class AuditDetailDrawerComponent {
  readonly entry = input<AuditEntry | null>(null);
  readonly open = input(false);
  readonly closed = output<void>();
}
