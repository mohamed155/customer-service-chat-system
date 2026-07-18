import { ChangeDetectionStrategy, Component, inject, input, signal } from '@angular/core';
import { catchError, finalize, of } from 'rxjs';
import { ConversationsApiService } from './conversations-api.service';
import { ButtonComponent } from '../../../shared/components/button/button.component';

@Component({
  selector: 'app-conversation-summary',
  standalone: true,
  imports: [ButtonComponent],
  template: `
    <div class="summary-panel">
      <div class="summary-header">
        <span class="summary-label">Conversation Summary</span>
        @if (!summary() && !loading()) {
          <app-button variant="secondary" size="sm" (pressed)="generate()"> Summarize </app-button>
        }
        @if (loading()) {
          <span class="generating">Generating…</span>
        }
      </div>
      @if (summary(); as s) {
        <p class="summary-text">{{ s }}</p>
      }
      @if (error(); as err) {
        <p class="summary-error">{{ err }}</p>
      }
    </div>
  `,
  styles: [
    `
      .summary-panel {
        padding: var(--app-space-3) var(--app-space-4);
        margin: 0 var(--app-space-4) var(--app-space-3);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        border: 1px solid var(--app-border);
      }
      .summary-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-2);
      }
      .summary-label {
        font-size: var(--app-font-sm);
        font-weight: 700;
        color: var(--app-text);
        text-transform: uppercase;
        letter-spacing: 0.05em;
      }
      .generating {
        font-size: var(--app-font-sm);
        color: var(--app-text-2);
        font-weight: 600;
      }
      .summary-text {
        margin: var(--app-space-2) 0 0;
        font-size: var(--app-font-sm);
        color: var(--app-text);
        line-height: 1.5;
        white-space: pre-wrap;
      }
      .summary-error {
        margin: var(--app-space-2) 0 0;
        font-size: var(--app-font-sm);
        color: var(--app-red);
        font-weight: 600;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ConversationSummaryComponent {
  readonly conversationId = input.required<string>();

  private readonly api = inject(ConversationsApiService);

  protected readonly summary = signal<string | null>(null);
  protected readonly loading = signal(false);
  protected readonly error = signal<string | null>(null);

  protected generate(): void {
    this.loading.set(true);
    this.error.set(null);
    this.api
      .requestSummary(this.conversationId())
      .pipe(
        catchError((err) => {
          this.error.set(err.message ?? 'Failed to generate summary');
          return of(null);
        }),
        finalize(() => this.loading.set(false)),
      )
      .subscribe((result) => {
        if (result) {
          this.summary.set(result.data.summary);
        }
      });
  }
}
