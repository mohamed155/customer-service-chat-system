import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { MessageAttachmentComponent } from './message-attachment.component';
import { AvatarComponent } from '../../../shared/components/avatar/avatar.component';
import { CitationListComponent } from '../../../shared/components/citation-list/citation-list.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { ToolApprovalCardComponent } from '../../../shared/components/tool-approval-card/tool-approval-card.component';
import { ToolTimelineEntryComponent } from '../../../shared/components/tool-timeline-entry/tool-timeline-entry.component';
import { Message, ToolRequest } from '../../../core/api/tenant-api.models';
import { ActiveGeneration } from './conversation-detail.store';
import { AiThinkingIndicatorComponent } from '../../../shared/components/ai/ai-thinking-indicator/ai-thinking-indicator.component';
import { AiConfidenceBadgeComponent } from '../../../shared/components/ai-confidence-badge/ai-confidence-badge.component';

type TimelineItem = { kind: 'message'; message: Message } | { kind: 'tool'; tool: ToolRequest };

@Component({
  selector: 'app-conversation-thread',
  imports: [
    MessageAttachmentComponent,
    AvatarComponent,
    CitationListComponent,
    LoadingStateComponent,
    AiThinkingIndicatorComponent,
    AiConfidenceBadgeComponent,
    ToolTimelineEntryComponent,
    ToolApprovalCardComponent,
  ],
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

      @for (
        item of timelineEntries();
        track item.kind + '-' + (item.kind === 'message' ? item.message.id : item.tool.id)
      ) {
        @if (item.kind === 'message') {
          <article
            [class]="senderClass(item.message)"
            [class.note-message]="item.message.kind === 'note'"
          >
            <app-avatar [initials]="avatarInitials(item.message)" size="sm" />
            <div class="bubble">
              <div class="bubble-header">
                <span class="sender-name">{{ item.message.sender.displayName }}</span>
                <span class="message-time">{{ formatTime(item.message.createdAt) }}</span>
                @if (item.message.kind === 'note') {
                  <span class="note-badge">Note</span>
                }
              </div>
              <p>{{ item.message.body }}</p>
              @if (item.message.attachments?.length) {
                @for (att of item.message.attachments; track att.id) {
                  <app-message-attachment [attachment]="att" />
                }
              }
              @if (item.message.citations?.length) {
                <app-citation-list [citations]="item.message.citations" />
              }
              @if (item.message.kind === 'ai' && item.message.confidence; as conf) {
                <app-ai-confidence-badge [band]="conf.band" />
              }
              @if (item.message.delivery; as delivery) {
                <span class="delivery-status" [class]="'status-' + delivery.status"
                      [title]="delivery.failureReason ?? ''">
                  @switch (delivery.status) {
                    @case ('pending') { 🕐 }
                    @case ('sent') { ✓ }
                    @case ('delivered') { ✓✓ }
                    @case ('read') { <span class="read-tick">✓✓</span> }
                    @case ('failed') { <span class="failed-icon">⚠</span> }
                  }
                </span>
              }
            </div>
          </article>
        } @else if (item.tool.status === 'awaiting_approval') {
          <app-tool-approval-card [request]="item.tool" />
        } @else {
          <app-tool-timeline-entry [request]="item.tool" />
        }
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
      .delivery-status {
        display: inline-flex;
        align-items: center;
        margin-left: var(--app-space-2);
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
      }
      .status-read {
        color: var(--app-accent);
      }
      .failed-icon {
        color: var(--app-red);
        cursor: help;
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
  readonly toolActivity = input<Record<string, ToolRequest>>({});
  readonly loadOlder = output<void>();

  protected readonly timelineEntries = computed<TimelineItem[]>(() => {
    const msgs = this.messages();
    const tools = Object.values(this.toolActivity());
    const all: TimelineItem[] = [
      ...msgs.map((m) => ({ kind: 'message' as const, message: m })),
      ...tools.map((t) => ({ kind: 'tool' as const, tool: t })),
    ];
    all.sort((a, b) => {
      const aTime = new Date(
        a.kind === 'message' ? a.message.createdAt : a.tool.createdAt,
      ).getTime();
      const bTime = new Date(
        b.kind === 'message' ? b.message.createdAt : b.tool.createdAt,
      ).getTime();
      const diff = aTime - bTime;
      if (diff !== 0) return diff;
      if (a.kind === 'tool' && b.kind === 'tool') {
        return a.tool.chainIndex - b.tool.chainIndex;
      }
      return a.kind === 'tool' ? -1 : 1;
    });
    return all;
  });

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
