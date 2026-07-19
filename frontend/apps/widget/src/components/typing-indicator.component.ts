import { Component, ChangeDetectionStrategy } from '@angular/core';

@Component({
  selector: 'hx-typing-indicator',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="typing" role="status" aria-label="Assistant is typing">
      <span class="typing__dot"></span>
      <span class="typing__dot"></span>
      <span class="typing__dot"></span>
    </div>
  `,
  styles: [
    `
      .typing {
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 10px 14px;
        background: var(--wgt-bubble-assistant);
        border-radius: 12px;
        align-self: flex-start;
        margin: 4px 0;
      }
      .typing__dot {
        width: 8px;
        height: 8px;
        border-radius: 50%;
        background: var(--wgt-muted-text);
        animation: typing-bounce 1.4s ease-in-out infinite;
      }
      .typing__dot:nth-child(2) {
        animation-delay: 0.2s;
      }
      .typing__dot:nth-child(3) {
        animation-delay: 0.4s;
      }
      @keyframes typing-bounce {
        0%,
        60%,
        100% {
          transform: translateY(0);
        }
        30% {
          transform: translateY(-6px);
        }
      }
      @media (prefers-reduced-motion: reduce) {
        .typing__dot {
          animation: none;
          opacity: 0.4;
        }
        .typing__dot:nth-child(2) {
          opacity: 0.6;
        }
        .typing__dot:nth-child(3) {
          opacity: 0.8;
        }
      }
    `,
  ],
})
export class TypingIndicatorComponent {}
