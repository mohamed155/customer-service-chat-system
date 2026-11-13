import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

@Component({
  selector: 'app-ai-handling-banner',
  standalone: true,
  providers: [provideTranslocoScope('conversations')],
  imports: [ButtonComponent, TranslocoPipe],
  templateUrl: './ai-handling-banner.component.html',
  styles: [
    `
      .ai-handling-banner {
        padding: var(--app-space-3) var(--app-space-4);
        margin: 0 var(--app-space-4) var(--app-space-3);
        border-radius: var(--app-radius-md);
        background: var(--app-warning-bg, #fff3cd);
        border: 1px solid var(--app-warning-border, #ffc107);
      }
      .ai-handling-banner p {
        margin: 0 0 var(--app-space-2);
        font-size: var(--app-font-sm);
        color: var(--app-text);
        font-weight: 600;
      }
      .actions {
        display: flex;
        gap: var(--app-space-2);
      }
      .reason {
        margin-top: var(--app-space-2);
        font-weight: 400;
        color: var(--app-text-2);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AiHandlingBannerComponent {
  readonly visible = input(false);
  readonly platformAiUnavailable = input(false);
  readonly platformAiUnavailableReason = input<string | null>(null);

  readonly choosePlatformAi = output<void>();
  readonly chooseHuman = output<void>();
}
