import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { LocaleDatePipe } from '../../../../core/i18n/format';
import { ButtonComponent } from '../../../../shared/components/button/button.component';
import { DialogShellComponent } from '../../../../shared/components/dialog-shell/dialog-shell.component';
import { InlineAlertComponent } from '../../../../shared/components/inline-alert/inline-alert.component';
import { PromptStore } from './prompt.store';
import { diffLines } from './prompt-lang';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

@Component({
  selector: 'app-version-history-drawer',
  standalone: true,
  imports: [
    ButtonComponent,
    DialogShellComponent,
    InlineAlertComponent,
    LocaleDatePipe,
    TranslocoPipe,
  ],
  providers: [provideTranslocoScope('ai-agent')],
  templateUrl: './version-history-drawer.component.html',
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
