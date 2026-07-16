import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { ButtonComponent } from '../../../shared/components/button/button.component';

@Component({
  selector: 'app-ai-handling-banner',
  standalone: true,
  imports: [ButtonComponent],
  template: `
    @if (visible()) {
      <div class="ai-handling-banner">
        <p>This conversation is awaiting an AI-handling decision.</p>
        <div class="actions">
          <app-button (pressed)="choosePlatformAi.emit()" [disabled]="platformAiUnavailable()">
            Use Platform AI
          </app-button>
          <app-button variant="secondary" (pressed)="chooseHuman.emit()">
            Assign to a Human
          </app-button>
        </div>
        @if (platformAiUnavailableReason(); as reason) {
          <p class="reason">{{ reason }}</p>
        }
      </div>
    }
  `,
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
