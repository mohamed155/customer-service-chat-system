import { Component, Input, ChangeDetectionStrategy } from '@angular/core';
import { WidgetMessage } from '../core/models';

@Component({
  selector: 'hx-message-bubble',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div
      class="bubble"
      [class.bubble--visitor]="message.sender === 'visitor'"
      [class.bubble--assistant]="message.sender === 'assistant'"
      [class.bubble--agent]="message.sender === 'agent'"
      [class.bubble--system]="message.sender === 'system'"
    >
      @if (message.sender === 'agent' && message.senderDisplayName) {
        <div class="bubble__name">{{ message.senderDisplayName }}</div>
      }
      <div class="bubble__text">{{ message.body }}</div>
    </div>
  `,
  styles: [
    `
      .bubble {
        max-width: 80%;
        padding: 10px 14px;
        border-radius: 12px;
        font-size: 14px;
        line-height: 1.4;
        word-wrap: break-word;
        margin: 4px 0;
      }
      .bubble--visitor {
        background: var(--wgt-bubble-visitor);
        color: #fff;
        align-self: flex-end;
        border-bottom-right-radius: 4px;
      }
      .bubble--assistant {
        background: var(--wgt-bubble-assistant);
        color: var(--wgt-text);
        align-self: flex-start;
        border-bottom-left-radius: 4px;
      }
      .bubble--agent {
        background: var(--wgt-bubble-assistant);
        color: var(--wgt-text);
        align-self: flex-start;
        border-bottom-left-radius: 4px;
      }
      .bubble--system {
        background: transparent;
        color: var(--wgt-muted-text);
        align-self: center;
        font-style: italic;
        font-size: 12px;
      }
      .bubble__name {
        font-size: 11px;
        font-weight: 600;
        color: var(--wgt-muted-text);
        margin-bottom: 2px;
      }
      .bubble__text {
        white-space: pre-wrap;
      }
    `,
  ],
})
export class MessageBubbleComponent {
  @Input({ required: true }) message!: WidgetMessage;
}
