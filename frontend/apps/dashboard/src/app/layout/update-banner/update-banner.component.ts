import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AppUpdateService } from '../../core/version/app-update.service';
import { TranslocoPipe } from '@jsverse/transloco';

/** The notice that a newer release is out (MOH-167). Sits in the app root, so every page has it. */
@Component({
  imports: [TranslocoPipe],
  selector: 'app-update-banner',
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './update-banner.component.html',
  styles: [
    `
      .update-banner {
        position: fixed;
        inset: auto var(--app-space-4, 16px) var(--app-space-4, 16px) auto;
        z-index: 1000;
        display: flex;
        align-items: center;
        gap: var(--app-space-3, 12px);
        max-width: min(32rem, calc(100vw - 2 * var(--app-space-4, 16px)));
        padding: var(--app-space-3, 12px) var(--app-space-4, 16px);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md, 8px);
        background: var(--app-panel);
        color: var(--app-text);
        font-size: var(--app-font-sm, 14px);
        box-shadow: var(--app-shadow-lg);
      }
      .update-banner span {
        flex: 1;
      }
      button {
        border: 0;
        background: transparent;
        color: var(--app-accent);
        cursor: pointer;
        font: inherit;
        font-weight: 700;
      }
      button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
      }
    `,
  ],
})
export class UpdateBannerComponent {
  protected readonly update = inject(AppUpdateService);
}
