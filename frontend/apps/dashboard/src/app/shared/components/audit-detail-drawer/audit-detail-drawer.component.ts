import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { DatePipe, JsonPipe } from '@angular/common';
import { AuditEntry } from '../../../core/api/tenant-api.models';
import { DialogShellComponent } from '../dialog-shell/dialog-shell.component';

@Component({
  selector: 'app-audit-detail-drawer',
  standalone: true,
  imports: [DatePipe, JsonPipe, DialogShellComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-dialog-shell variant="drawer-right" [open]="open()" (dismiss)="closed.emit()">
      @if (entry(); as e) {
        <div class="drawer-header">
          <h2 class="drawer-title">Audit Entry</h2>
          <button type="button" class="close-btn" (click)="closed.emit()" aria-label="Close">
            &times;
          </button>
        </div>

        <dl class="def-list">
          <div class="def-row">
            <dt>Time</dt>
            <dd>{{ e.createdAt | date: 'medium' }}</dd>
          </div>
          <div class="def-row">
            <dt>Actor</dt>
            <dd>
              @if (e.actor.kind === 'system') {
                <span>System</span>
              } @else {
                <span>{{ e.actor.displayName }}</span>
                @if (e.actor.isPlatformStaff) {
                  <span class="badge badge-staff">Platform staff</span>
                }
                @if (e.actor.deleted) {
                  <span class="badge badge-deleted">deleted</span>
                }
              }
            </dd>
          </div>
          <div class="def-row">
            <dt>Action</dt>
            <dd>
              <code>{{ e.action }}</code>
            </dd>
          </div>
          <div class="def-row">
            <dt>Category</dt>
            <dd>{{ e.category }}</dd>
          </div>
          <div class="def-row">
            <dt>Target type</dt>
            <dd>{{ e.resourceType }}</dd>
          </div>
          <div class="def-row">
            <dt>Target ID</dt>
            <dd>
              <code>{{ e.resourceId }}</code>
            </dd>
          </div>
          @if (e.tenantId) {
            <div class="def-row">
              <dt>Tenant</dt>
              <dd>
                <code>{{ e.tenantId }}</code>
              </dd>
            </div>
          }
        </dl>

        <h3 class="meta-heading">Metadata</h3>
        <pre class="meta-json">{{ e.details | json }}</pre>
      }
    </app-dialog-shell>
  `,
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
