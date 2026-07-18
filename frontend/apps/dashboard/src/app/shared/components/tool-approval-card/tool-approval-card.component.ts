import { JsonPipe } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { ToolRequest } from '../../../core/api/tenant-api.models';

@Component({
  imports: [JsonPipe],
  selector: 'app-tool-approval-card',
  template: `
    <article class="approval-card" [class.resolved]="isResolved()">
      <div class="card-header">
        <span class="tool-name">{{ request().toolName }}</span>
        <span class="status-badge" [class.active]="!isResolved()" [class.settled]="isResolved()">
          {{ isResolved() ? request().status : 'pending approval' }}
        </span>
      </div>

      <div class="card-body">
        <div class="args-section">
          <span class="label">Arguments:</span>
          <pre class="args-json">{{ request().arguments | json }}</pre>
        </div>

        @if (request().expiresAt && !isResolved()) {
          <div class="expiry-section">
            <span class="label">Expires:</span>
            <span class="countdown">{{ expiryText() }}</span>
          </div>
        }
      </div>

      @if (!isResolved()) {
        <div class="card-actions">
          <button
            type="button"
            class="btn btn-approve"
            (click)="onApprove()"
            [disabled]="actionInProgress()"
          >
            Approve
          </button>
          <button
            type="button"
            class="btn btn-deny"
            (click)="onDeny()"
            [disabled]="actionInProgress()"
          >
            Deny
          </button>
        </div>
      } @else {
        <div class="resolved-state">
          @if (request().status === 'approved') {
            <span class="resolved-text approved">Approved — tool will execute</span>
          } @else if (request().status === 'denied') {
            <span class="resolved-text denied">Denied</span>
          } @else if (request().status === 'expired') {
            <span class="resolved-text expired">Expired — no decision was made in time</span>
          } @else if (request().status === 'cancelled') {
            <span class="resolved-text cancelled">Cancelled</span>
          } @else if (request().status === 'succeeded') {
            <span class="resolved-text succeeded">Executed successfully</span>
          } @else if (request().status === 'failed' || request().status === 'timed_out') {
            <span class="resolved-text failed">Execution failed</span>
          } @else {
            <span class="resolved-text">{{ request().status }}</span>
          }
        </div>
      }

      @if (request().error; as err) {
        <p class="error-detail">{{ err }}</p>
      }
    </article>
  `,
  styles: [
    `
      :host {
        display: block;
      }
      .approval-card {
        display: grid;
        gap: var(--app-space-3);
        padding: var(--app-space-4);
        border: 2px solid var(--app-amber);
        border-radius: var(--app-radius-lg);
        background: var(--app-amber-soft);
        max-width: 82%;
      }
      .approval-card.resolved {
        border-color: var(--app-border);
        background: var(--app-panel-2);
      }
      .card-header {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
      }
      .tool-name {
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 700;
        font-family: var(--app-font-mono, monospace);
      }
      .status-badge {
        display: inline-flex;
        padding: 2px 8px;
        border-radius: 999px;
        font-size: var(--app-font-xs);
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.03em;
      }
      .status-badge.active {
        background: var(--app-amber-soft);
        color: #000;
      }
      .status-badge.settled {
        background: var(--app-panel);
        color: var(--app-text-3);
      }
      .card-body {
        display: grid;
        gap: var(--app-space-2);
      }
      .label {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.04em;
      }
      .args-section {
        display: grid;
        gap: 4px;
      }
      .args-json {
        margin: 0;
        padding: var(--app-space-2);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius);
        background: var(--app-panel);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        font-family: var(--app-font-mono, monospace);
        white-space: pre-wrap;
        word-break: break-word;
      }
      .expiry-section {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
      }
      .countdown {
        color: var(--app-red);
        font-size: var(--app-font-xs);
        font-family: var(--app-font-mono, monospace);
        font-weight: 600;
      }
      .card-actions {
        display: flex;
        gap: var(--app-space-2);
      }
      .btn {
        height: 34px;
        padding: 0 var(--app-space-4);
        border: none;
        border-radius: var(--app-radius);
        font-size: var(--app-font-sm);
        font-weight: 600;
        cursor: pointer;
      }
      .btn:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
      .btn-approve {
        background: var(--app-green);
        color: #000;
      }
      .btn-approve:hover:not(:disabled) {
        opacity: 0.85;
      }
      .btn-deny {
        background: var(--app-red);
        color: var(--app-text);
      }
      .btn-deny:hover:not(:disabled) {
        opacity: 0.85;
      }
      .resolved-state {
        padding: var(--app-space-2) var(--app-space-3);
        border-radius: var(--app-radius);
        background: var(--app-panel);
      }
      .resolved-text {
        font-size: var(--app-font-xs);
        font-weight: 600;
      }
      .resolved-text.approved {
        color: var(--app-green);
      }
      .resolved-text.denied {
        color: var(--app-red);
      }
      .resolved-text.expired {
        color: var(--app-text-3);
      }
      .resolved-text.cancelled {
        color: var(--app-text-3);
      }
      .resolved-text.succeeded {
        color: var(--app-green);
      }
      .resolved-text.failed {
        color: var(--app-red);
      }
      .error-detail {
        margin: 0;
        color: var(--app-red);
        font-size: var(--app-font-xs);
        line-height: 1.4;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolApprovalCardComponent {
  readonly request = input.required<ToolRequest>();

  readonly approve = output<string>();
  readonly deny = output<string>();

  protected readonly actionInProgress = computed(() => false);

  protected readonly isResolved = computed(() => this.request().status !== 'awaiting_approval');

  protected readonly expiryText = computed(() => {
    const expiresAt = this.request().expiresAt;
    if (!expiresAt) return '';
    const remaining = new Date(expiresAt).getTime() - Date.now();
    if (remaining <= 0) return 'Expired';
    const mins = Math.floor(remaining / 60000);
    const secs = Math.floor((remaining % 60000) / 1000);
    return `${mins}m ${secs}s`;
  });

  protected onApprove(): void {
    this.approve.emit(this.request().id);
  }

  protected onDeny(): void {
    this.deny.emit(this.request().id);
  }
}
