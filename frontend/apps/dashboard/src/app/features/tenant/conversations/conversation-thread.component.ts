import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { CitationListComponent } from '../../../shared/components/citation-list/citation-list.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { Message } from '../../../core/api/tenant-api.models';
import { ActiveGeneration } from './conversation-detail.store';
import { AiThinkingIndicatorComponent } from '../../../shared/components/ai/ai-thinking-indicator/ai-thinking-indicator.component';
import { AiConfidenceBadgeComponent } from '../../../shared/components/ai-confidence-badge/ai-confidence-badge.component';

@Component({
  selector: 'app-conversation-thread',
  imports: [AvatarComponent, CitationListComponent, LoadingStateComponent, AiThinkingIndicatorComponent, AiConfidenceBadgeComponent],
  template: `
    <div class="messages" #scrollContainer>
      @if (hasMore()) {
        <div class="load-older-wrap">
          <button type="button" class="load-older-btn" (click)="loadOlder.emit()">
            Load older messages
          </button>
        </div>
      }

      @if (loading()) {
        <app-loading-state label="Loading messages" />
      }

      @for (message of messages(); track message.id) {
        <article [class]="senderClass(message)" [class.note-message]="message.kind === 'note'">
          <app-avatar [initials]="avatarInitials(message)" size="sm" />
          <div class="bubble">
            <div class="bubble-header">
              <span class="sender-name">{{ message.sender.displayName }}</span>
              <span class="message-time">{{ formatTime(message.createdAt) }}</span>
              @if (message.kind === 'note') {
                <span class="note-badge">Note</span>
              }
            </div>
            <p>{{ message.body }}</p>
            @if (message.citations?.length) {
              <app-citation-list [citations]="message.citations" />
            }
            @if (message.kind === 'ai' && message.confidence; as conf) {
              <app-ai-confidence-badge [band]="conf.band" />
            }
          </div>
        </article>
      }

      @if (activeGeneration(); as gen) {
        @if (gen.phase === 'thinking') {
          <app-ai-thinking-indicator />
        } @else if (gen.phase === 'streaming') {
          <article class="member ai-streaming">
            <app-avatar initials="AI" size="sm" />
            <div class="bubble">
              <div class="bubble-header">
                <span class="sender-name">AI Assistant</span>
                <span class="message-time">streaming…</span>
              </div>
              <p>{{ gen.buffer }}</p>
            </div>
          </article>
        }
      }

      @if (messages().length === 0 && !activeGeneration() && !loading()) {
        <div class="empty-timeline">No messages yet</div>
      }
    </div>
  `,
  styles: [
    `
      :host {
        min-height: 0;
        display: block;
        overflow-y: auto;
        background: var(--app-panel);
      }
      .messages {
        min-height: 0;
        display: grid;
        align-content: start;
        gap: var(--app-space-4);
        padding: var(--app-space-5);
      }
      .load-older-wrap {
        display: flex;
        justify-content: center;
        padding-bottom: var(--app-space-3);
      }
      .load-older-btn {
        height: 34px;
        padding: 0 var(--app-space-4);
        border: 1px solid var(--app-border);
        border-radius: 999px;
        background: var(--app-panel);
        color: var(--app-text-2);
        font-weight: 600;
        font-size: var(--app-font-sm);
        cursor: pointer;
      }
      .load-older-btn:hover {
        background: var(--app-panel-2);
        color: var(--app-text);
      }
      article {
        display: flex;
        gap: var(--app-space-3);
        max-width: 82%;
      }
      article.member {
        justify-self: end;
        flex-direction: row-reverse;
      }
      article.note-message {
        max-width: 100%;
        justify-self: center;
      }
      article.ai-streaming .bubble {
        background: var(--app-accent-soft);
        border-color: var(--app-accent);
        opacity: 0.85;
      }
      article.note-message .bubble {
        background: var(--app-amber-soft);
        border-color: transparent;
      }
      .bubble {
        padding: var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel-2);
        min-width: 0;
      }
      .member .bubble {
        background: var(--app-accent-soft);
        border-color: transparent;
      }
      .bubble-header {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        margin-bottom: 6px;
      }
      .sender-name {
        color: var(--app-text);
        font-size: var(--app-font-xs);
        font-weight: 700;
      }
      .message-time {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .note-badge {
        display: inline-flex;
        padding: 0 6px;
        border-radius: 999px;
        background: var(--app-amber);
        color: #000;
        font-size: 10px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.04em;
      }
      p {
        margin: 0;
        color: var(--app-text);
        font-size: var(--app-font-sm);
        line-height: 1.55;
        white-space: pre-wrap;
        word-break: break-word;
      }
      .empty-timeline {
        display: grid;
        place-items: center;
        padding: var(--app-space-8);
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
      }
      @media (max-width: 768px) {
        article {
          max-width: 100%;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ConversationThreadComponent {
  readonly messages = input.required<readonly Message[]>();
  readonly loading = input(false);
  readonly hasMore = input(false);
  readonly activeGeneration = input<ActiveGeneration | null>(null);
  readonly loadOlder = output<void>();

  protected senderClass(message: Message): string {
    return message.sender.type;
  }

  protected avatarInitials(message: Message): string {
    if (message.kind === 'note') return 'NN';
    return message.sender.displayName
      .split(' ')
      .map((p) => p[0])
      .join('')
      .toUpperCase()
      .slice(0, 2);
  }

  protected formatTime(iso: string): string {
    const date = new Date(iso);
    const hours = date.getHours().toString().padStart(2, '0');
    const minutes = date.getMinutes().toString().padStart(2, '0');
    return `${hours}:${minutes}`;
  }
}
