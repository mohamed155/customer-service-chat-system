import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { AiConfidenceBadgeComponent } from '../../../shared/components/ai/ai-confidence-badge/ai-confidence-badge.component';
import { AiSuggestionCardComponent } from '../../../shared/components/ai/ai-suggestion-card/ai-suggestion-card.component';
import { KnowledgeCitationComponent } from '../../../shared/components/ai/knowledge-citation/knowledge-citation.component';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { ChannelBadgeComponent } from '../../../shared/components/channel-badge/channel-badge.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import {
  CustomerFixture,
  ConversationFixture,
  MessageFixture,
} from '../../../shared/fixtures/fixture.models';

@Component({
  selector: 'app-conversation-thread',
  imports: [
    AiConfidenceBadgeComponent,
    AiSuggestionCardComponent,
    AvatarComponent,
    ChannelBadgeComponent,
    KnowledgeCitationComponent,
    StatusBadgeComponent,
  ],
  template: `
    @if (conversation(); as thread) {
      <div class="thread-header">
        <div>
          <strong>{{ customer()?.name ?? 'Customer' }}</strong>
          <span>{{ customer()?.email }}</span>
        </div>
        <div class="badges">
          <app-channel-badge [channel]="thread.channel" />
          <app-status-badge [status]="thread.status" [tone]="statusTone(thread.status)" />
        </div>
      </div>

      <div class="messages">
        @for (message of thread.messages; track message.id) {
          @if (message.author === 'system') {
            <div class="system">{{ message.body }}</div>
          } @else {
            <article [class]="message.author">
              <app-avatar [initials]="avatarInitials(message)" size="sm" />
              <div class="bubble">
                <p>{{ message.body }}</p>
                @if (message.author === 'ai') {
                  <div class="ai-meta">
                    <app-ai-confidence-badge [confidence]="message.aiConfidence ?? 0" />
                    @if (message.citations?.length) {
                      <app-knowledge-citation [titles]="message.citations" />
                    }
                  </div>
                }
              </div>
            </article>
          }
        }
      </div>

      <footer>
        <app-ai-suggestion-card
          suggestion="I can summarize the issue, confirm next steps, and keep the customer updated while an agent reviews the edge case."
        >
          <button type="button">Use reply</button>
          <button type="button">Edit</button>
        </app-ai-suggestion-card>
        <textarea aria-label="Reply composer" placeholder="Reply to customer..."></textarea>
        <div class="composer-actions">
          <button type="button">
            {{ thread.status === 'escalated' ? 'Hand back to AI' : 'Take over' }}
          </button>
          <button type="button" class="send">Send</button>
        </div>
      </footer>
    }
  `,
  styles: [
    `
      :host {
        min-height: 0;
        display: grid;
        grid-template-rows: auto 1fr auto;
        background: var(--app-panel);
      }
      .thread-header {
        display: flex;
        justify-content: space-between;
        gap: var(--app-space-3);
        padding: var(--app-space-4);
        border-bottom: 1px solid var(--app-border);
      }
      .thread-header strong {
        display: block;
        color: var(--app-text);
        font-size: var(--app-font-base);
      }
      .thread-header span {
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
      }
      .badges,
      .ai-meta,
      .composer-actions {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        flex-wrap: wrap;
      }
      .messages {
        min-height: 0;
        display: grid;
        align-content: start;
        gap: var(--app-space-4);
        overflow-y: auto;
        padding: var(--app-space-5);
      }
      article {
        display: flex;
        gap: var(--app-space-3);
        max-width: 78%;
      }
      article.ai,
      article.human {
        justify-self: end;
        flex-direction: row-reverse;
      }
      .bubble {
        padding: var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel-2);
      }
      .ai .bubble {
        background: var(--app-accent-soft);
      }
      .human .bubble {
        background: var(--app-panel-3);
      }
      p {
        margin: 0;
        color: var(--app-text);
        font-size: var(--app-font-sm);
        line-height: 1.5;
      }
      .ai-meta {
        margin-top: var(--app-space-3);
      }
      .system {
        justify-self: center;
        padding: 6px 10px;
        border-radius: 999px;
        background: var(--app-amber-soft);
        color: var(--app-amber);
        font-size: var(--app-font-xs);
        font-weight: 650;
      }
      footer {
        display: grid;
        gap: var(--app-space-3);
        padding: var(--app-space-4);
        border-top: 1px solid var(--app-border);
      }
      textarea {
        min-height: 72px;
        resize: vertical;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel-2);
        color: var(--app-text);
        padding: var(--app-space-3);
        font: inherit;
      }
      button {
        height: 34px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font-weight: 650;
        cursor: pointer;
      }
      .send {
        margin-left: auto;
        border-color: var(--app-accent);
        background: var(--app-accent);
        color: var(--app-accent-ink);
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
  readonly conversation = input<ConversationFixture | null>(null);
  readonly customer = input<CustomerFixture | null>(null);

  protected avatarInitials(message: MessageFixture): string {
    if (message.author === 'ai') return 'AI';
    if (message.author === 'human') return 'NF';
    return this.customer()?.avatarInitials ?? 'HC';
  }

  protected statusTone(status: ConversationFixture['status']): 'green' | 'amber' | 'red' {
    return status === 'closed' ? 'green' : status === 'escalated' ? 'red' : 'amber';
  }
}
