import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { DatePipe } from '@angular/common';
import { ButtonComponent } from '../../../../shared/components/button/button.component';
import { DialogShellComponent } from '../../../../shared/components/dialog-shell/dialog-shell.component';
import { InlineAlertComponent } from '../../../../shared/components/inline-alert/inline-alert.component';
import { PromptStore } from './prompt.store';
import { diffLines } from './prompt-lang';

@Component({
  selector: 'app-version-history-drawer',
  standalone: true,
  imports: [ButtonComponent, DialogShellComponent, InlineAlertComponent],
  providers: [DatePipe],
  template: `
    <app-dialog-shell variant="drawer-right" [open]="open()" (dismiss)="closed.emit()">
      <div class="drawer-header">
        <h2 class="drawer-title">Version History</h2>
        <button type="button" class="close-btn" (click)="closed.emit()" aria-label="Close">
          &times;
        </button>
      </div>

      @if (store.historyLoading() && store.historyItems().length === 0) {
        <div class="loading-hint">Loading…</div>
      }

      @if (store.selectedVersion(); as version) {
        <div class="detail-view">
          <button type="button" class="back-btn" (click)="backToList()">&larr; Back to list</button>
          <div class="detail-header">
            <span class="version-badge">Version {{ version.versionNumber }}</span>
            @if (version.isActive) {
              <span class="active-badge">Active</span>
            }
            @if (version.restoredFrom) {
              <span class="restored-badge">Restored from v{{ version.restoredFrom }}</span>
            }
          </div>
          <div class="detail-meta">
            <span>{{ version.createdBy }}</span>
            <span>{{ datePipe.transform(version.createdAt, 'medium') }}</span>
          </div>
          <pre class="detail-content">{{ version.content }}</pre>

          @if (diffResult(); as diff) {
            <div class="diff-section">
              <h3 class="diff-title">Changes from active</h3>
              @for (line of diff; track $index) {
                <div
                  class="diff-line"
                  [class.diff-added]="line.kind === 'added'"
                  [class.diff-removed]="line.kind === 'removed'"
                >
                  @if (line.kind === 'added') {
                    <span class="diff-prefix">+</span>
                    <span class="diff-label">added</span>
                  } @else if (line.kind === 'removed') {
                    <span class="diff-prefix">−</span>
                    <span class="diff-label">removed</span>
                  } @else {
                    <span class="diff-prefix"> </span>
                  }
                  <span class="diff-text">{{ line.text }}</span>
                </div>
              }
            </div>
          }

          @if (store.saving()) {
            <div class="saving-overlay">Restoring…</div>
          } @else {
            @if (confirmingVersion() === version.versionNumber) {
              <div class="confirm-bar">
                <app-inline-alert tone="info"
                  >Restore version {{ version.versionNumber }} as the active
                  prompt?</app-inline-alert
                >
                <div class="confirm-actions">
                  <app-button
                    variant="danger"
                    size="sm"
                    (pressed)="store.restore(version.versionNumber)"
                    >Confirm Restore</app-button
                  >
                  <app-button variant="secondary" size="sm" (pressed)="cancelConfirm()"
                    >Cancel</app-button
                  >
                </div>
                @if (store.fieldErrors(); as errors) {
                  @for (msgs of getErrorValues(errors); track $index) {
                    @for (msg of msgs; track msg) {
                      <app-inline-alert tone="error">{{ msg }}</app-inline-alert>
                    }
                  }
                }
              </div>
            } @else {
              <app-button
                variant="primary"
                size="sm"
                (pressed)="requestRestore(version.versionNumber)"
                [disabled]="version.isActive"
              >
                {{ version.isActive ? 'Currently Active' : 'Restore This Version' }}
              </app-button>
            }
          }
        </div>
      } @else {
        <div class="list-view">
          @for (item of store.historyItems(); track item.versionNumber) {
            <button
              type="button"
              class="version-row"
              (click)="store.selectVersion(item.versionNumber)"
            >
              <div class="version-row-header">
                <span class="version-badge">v{{ item.versionNumber }}</span>
                @if (item.isActive) {
                  <span class="active-badge">Active</span>
                }
                @if (item.restoredFrom) {
                  <span class="restored-badge">Restored from v{{ item.restoredFrom }}</span>
                }
              </div>
              <div class="version-row-meta">
                <span>{{ item.createdBy }}</span>
                <span>{{ datePipe.transform(item.createdAt, 'medium') }}</span>
              </div>
              @if (item.changeNote; as note) {
                <p class="change-note">{{ note }}</p>
              }
              <p class="content-preview">{{ item.contentPreview }}</p>
            </button>
          } @empty {
            @if (!store.historyLoading()) {
              <div class="empty-state">No version history available.</div>
            }
          }

          @if (store.historyHasMore()) {
            <div class="load-more-wrap">
              <app-button
                variant="secondary"
                size="sm"
                (pressed)="store.loadHistory(lastVersionNumber())"
                [disabled]="store.historyLoading()"
              >
                {{ store.historyLoading() ? 'Loading…' : 'Load more' }}
              </app-button>
            </div>
          }
        </div>
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
      .loading-hint {
        padding: var(--app-space-4);
        text-align: center;
        color: var(--app-text-2);
      }
      .empty-state {
        padding: var(--app-space-4);
        text-align: center;
        color: var(--app-text-3);
      }

      .list-view {
        display: flex;
        flex-direction: column;
        gap: var(--app-space-2);
      }

      .version-row {
        display: block;
        width: 100%;
        text-align: left;
        padding: var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        cursor: pointer;
        font: inherit;
        color: inherit;
      }
      .version-row:hover {
        background: var(--app-panel-2);
      }
      .version-row:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
      .version-row-header {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        margin-bottom: var(--app-space-1);
      }
      .version-badge {
        font-weight: 650;
        font-size: var(--app-font-sm);
      }
      .active-badge {
        font-size: var(--app-font-xs);
        padding: 1px var(--app-space-2);
        border-radius: var(--app-radius-sm);
        background: var(--app-green-soft, #d4edda);
        color: var(--app-green, #155724);
        font-weight: 600;
      }
      .restored-badge {
        font-size: var(--app-font-xs);
        padding: 1px var(--app-space-2);
        border-radius: var(--app-radius-sm);
        background: var(--app-blue-soft, #d1ecf1);
        color: var(--app-blue, #0c5460);
        font-weight: 600;
      }
      .version-row-meta {
        display: flex;
        gap: var(--app-space-3);
        font-size: var(--app-font-xs);
        color: var(--app-text-2);
        margin-bottom: var(--app-space-1);
      }
      .change-note {
        margin: var(--app-space-1) 0;
        font-size: var(--app-font-sm);
        font-style: italic;
        color: var(--app-text-2);
      }
      .content-preview {
        margin: var(--app-space-1) 0 0;
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .load-more-wrap {
        display: flex;
        justify-content: center;
        padding: var(--app-space-2);
      }

      .detail-view {
        display: flex;
        flex-direction: column;
        gap: var(--app-space-3);
      }
      .back-btn {
        background: none;
        border: none;
        color: var(--app-accent);
        cursor: pointer;
        font: inherit;
        font-size: var(--app-font-sm);
        padding: 0;
        text-align: left;
      }
      .back-btn:hover {
        text-decoration: underline;
      }
      .detail-header {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
      }
      .detail-meta {
        display: flex;
        gap: var(--app-space-3);
        font-size: var(--app-font-xs);
        color: var(--app-text-2);
      }
      .detail-content {
        margin: 0;
        padding: var(--app-space-3);
        background: var(--app-bg, #f8f9fa);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        font-family: monospace;
        font-size: var(--app-font-xs);
        line-height: 1.6;
        white-space: pre-wrap;
        max-height: 40dvh;
        overflow: auto;
      }
      .diff-section {
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        padding: var(--app-space-3);
        font-family: monospace;
        font-size: var(--app-font-xs);
        line-height: 1.6;
        max-height: 30dvh;
        overflow: auto;
      }
      .diff-title {
        margin: 0 0 var(--app-space-2);
        font-size: var(--app-font-sm);
        font-weight: 650;
      }
      .diff-line {
        display: flex;
        gap: var(--app-space-2);
        white-space: pre-wrap;
      }
      .diff-added {
        background: #d4edda;
      }
      .diff-removed {
        background: #f8d7da;
      }
      .diff-prefix {
        flex-shrink: 0;
        width: 1em;
        user-select: none;
      }
      .diff-label {
        flex-shrink: 0;
        font-size: 0.7em;
        text-transform: uppercase;
        font-weight: 600;
        color: var(--app-text-2);
        min-width: 3.5em;
      }
      .saving-overlay {
        text-align: center;
        padding: var(--app-space-3);
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
      }
      .confirm-bar {
        display: flex;
        flex-direction: column;
        gap: var(--app-space-2);
      }
      .confirm-actions {
        display: flex;
        gap: var(--app-space-2);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class VersionHistoryDrawerComponent {
  protected readonly store = inject(PromptStore);
  protected readonly datePipe = inject(DatePipe);

  readonly open = input(false);
  readonly closed = output<void>();

  protected confirmingVersion = signal(0);

  protected diffResult = computed(() => {
    const version = this.store.selectedVersion();
    const bootstrap = this.store.bootstrap();
    if (!version || !bootstrap) return null;
    return diffLines(version.content, bootstrap.prompt.content);
  });

  protected getErrorValues(errors: Record<string, string[]>): string[][] {
    return Object.values(errors);
  }

  protected lastVersionNumber(): number | undefined {
    const items = this.store.historyItems();
    if (items.length === 0) return undefined;
    return items[items.length - 1].versionNumber;
  }

  protected requestRestore(versionNumber: number): void {
    this.confirmingVersion.set(versionNumber);
  }

  protected cancelConfirm(): void {
    this.confirmingVersion.set(0);
  }

  protected backToList(): void {
    this.store.clearSelectedVersion();
  }
}
