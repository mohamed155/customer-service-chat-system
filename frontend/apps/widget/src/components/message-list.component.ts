import {
  Component,
  ElementRef,
  Input,
  ViewChild,
  AfterViewChecked,
  ChangeDetectionStrategy,
} from '@angular/core';
import { WidgetMessage } from '../core/models';
import { MessageBubbleComponent } from './message-bubble.component';
import { TypingIndicatorComponent } from './typing-indicator.component';

@Component({
  selector: 'hx-message-list',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MessageBubbleComponent, TypingIndicatorComponent],
  template: `
    <div class="list" #scrollContainer aria-live="polite">
      @for (msg of messages; track msg.id) {
        <hx-message-bubble [message]="msg" />
      }
      @if (streamingText) {
        <hx-typing-indicator />
      }
    </div>
  `,
  styles: [
    `
      .list {
        flex: 1;
        overflow-y: auto;
        padding: 12px;
        display: flex;
        flex-direction: column;
        gap: 4px;
      }
    `,
  ],
})
export class MessageListComponent implements AfterViewChecked {
  @Input({ required: true }) messages!: WidgetMessage[];
  @Input() streamingText = '';

  @ViewChild('scrollContainer') private scrollContainer!: ElementRef<HTMLElement>;

  private nearBottom = true;

  ngAfterViewChecked(): void {
    const el = this.scrollContainer?.nativeElement;
    if (!el) return;
    const threshold = el.scrollHeight - el.clientHeight - 60;
    this.nearBottom = el.scrollTop >= threshold;
    if (this.nearBottom) {
      el.scrollTop = el.scrollHeight;
    }
  }
}
