import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';
import { shortReference } from '../../../core/errors/request-reference';
import { copyToClipboard } from '../../utils/clipboard';

/**
 * The short reference of a failed request, with a button that copies the full request ID: what a
 * person sends to support so the failure can be found in the backend's logs (MOH-64).
 */
@Component({
  selector: 'app-error-reference',
  templateUrl: './error-reference.component.html',
  styles: [
    `
      :host {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-2);
        font-size: var(--app-font-sm);
        font-weight: 400;
        color: var(--app-text-2);
      }
      code {
        font-family: var(--app-font-mono, monospace);
        user-select: all;
      }
      button {
        border: 0;
        background: none;
        padding: 0;
        font: inherit;
        color: inherit;
        text-decoration: underline;
        cursor: pointer;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ErrorReferenceComponent {
  /** The full request ID. */
  readonly requestId = input.required<string>();

  protected readonly reference = computed(() => shortReference(this.requestId()));
  protected readonly copied = signal<'idle' | 'copied' | 'failed'>('idle');

  protected async copy(): Promise<void> {
    try {
      await copyToClipboard(this.requestId());
      this.copied.set('copied');
    } catch {
      this.copied.set('failed');
    }
  }
}
