import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { IndexStatus } from '../../../core/api/knowledge.models';

@Component({
  selector: 'app-index-status-badge',
  host: { '[class]': 'hostClass()' },
  template: `
    @if (showSpinner()) {
      <span class="spinner"></span>
    }
    <span>{{ label() }}</span>
    @if (chunkCount(); as cnt) {
      <span class="chunks">({{ cnt }})</span>
    }
  `,
  styles: [
    `
      :host {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-1);
        min-height: 22px;
        padding: 0 var(--app-space-2);
        border-radius: 999px;
        font-size: var(--app-font-xs);
        font-weight: 600;
        line-height: 1;
        text-transform: capitalize;
        white-space: nowrap;
        cursor: default;
      }
      :host(.not_indexed) {
        background: var(--app-panel-2);
        color: var(--app-text-2);
      }
      :host(.pending) {
        background: var(--app-amber-soft);
        color: var(--app-amber);
      }
      :host(.indexing) {
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      :host(.indexed) {
        background: var(--app-green-soft);
        color: var(--app-green);
      }
      :host(.failed) {
        background: var(--app-red-soft);
        color: var(--app-red);
      }
      :host(.not_indexable) {
        background: var(--app-panel-2);
        color: var(--app-text-2);
      }
      .spinner {
        display: inline-block;
        width: 10px;
        height: 10px;
        border: 2px solid currentColor;
        border-top-color: transparent;
        border-radius: 50%;
        animation: spin 0.6s linear infinite;
      }
      @keyframes spin {
        to {
          transform: rotate(360deg);
        }
      }
      .chunks {
        opacity: 0.7;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class IndexStatusBadgeComponent {
  readonly indexStatus = input.required<IndexStatus>();

  protected readonly hostClass = computed(() => this.indexStatus().status);

  protected readonly showSpinner = computed(
    () => this.indexStatus().status === 'pending' || this.indexStatus().status === 'indexing',
  );

  protected readonly label = computed(() =>
    this.indexStatus()
      .status.replaceAll('_', ' ')
      .replace(/\b\w/g, (c) => c.toUpperCase()),
  );

  protected readonly chunkCount = computed(() => {
    const st = this.indexStatus();
    return st.status === 'indexed' ? st.chunkCount : null;
  });
}
