import { ChangeDetectionStrategy, Component } from '@angular/core';

@Component({
  selector: 'app-ai-thinking-indicator',
  template: `<span></span><span></span><span></span>`,
  styles: [
    `
      :host {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 8px 10px;
        border-radius: 999px;
        background: var(--app-panel-2);
      }
      span {
        width: 6px;
        height: 6px;
        border-radius: 999px;
        background: var(--app-accent);
        animation: pulse 900ms ease-in-out infinite;
      }
      span:nth-child(2) {
        animation-delay: 120ms;
      }
      span:nth-child(3) {
        animation-delay: 240ms;
      }
      @keyframes pulse {
        50% {
          opacity: 0.35;
          transform: translateY(-2px);
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AiThinkingIndicatorComponent {}
