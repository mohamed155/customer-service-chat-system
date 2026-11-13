import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AppUpdateService } from '../../core/version/app-update.service';

/** The notice that a newer release is out (MOH-167). Sits in the app root, so every page has it. */
@Component({
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
        border: 1px solid var(--app-border, #d0d5dd);
        border-radius: var(--app-radius-md, 8px);
        background: var(--app-panel, #fff);
        color: var(--app-text, #101828);
        font-size: var(--app-font-sm, 14px);
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
      }
      .update-banner span {
        flex: 1;
      }
      button {
        border: 0;
        background: transparent;
        color: var(--app-accent, #175cd3);
        cursor: pointer;
        font: inherit;
        font-weight: 700;
      }
      button:focus-visible {
        outline: 3px solid var(--app-accent-soft, #84adff);
        outline-offset: 2px;
      }
    `,
  ],
})
export class UpdateBannerComponent {
  protected readonly update = inject(AppUpdateService);
}
