import { ChangeDetectionStrategy, Component, input, output, signal, computed } from '@angular/core';
import { StarRatingComponent } from './star-rating.component';
import { WidgetFeedback } from '../core/models';

@Component({
  standalone: true,
  selector: 'wgt-feedback-prompt',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [StarRatingComponent],
  template: `
    @if (state() === 'prompt') {
      <div class="prompt">
        <p class="heading">How did we do?</p>
        <wgt-star-rating (rate)="onRate($event)" />
        <div class="comment-area">
          <textarea
            class="comment-input"
            placeholder="Tell us more (optional)"
            [value]="comment()"
            (input)="onInput($event)"
          ></textarea>
          <span class="char-counter" [class.over]="overLimit()">{{ charCount() }}/2000</span>
        </div>
        @if (selectedRating() >= 1) {
          <button type="button" class="submit-btn" [disabled]="overLimit()" (click)="onSubmit()">
            Send feedback
          </button>
        }
        <button type="button" class="dismiss-btn" (click)="dismiss.emit()">Not now</button>
      </div>
    } @else if (state() === 'collapsed') {
      <button type="button" class="collapsed-btn" (click)="expand.emit()">
        Rate this conversation
      </button>
    } @else if (state() === 'submitted' && feedback(); as fb) {
      <div class="submitted">
        <p class="thank-you">Thank you for your feedback!</p>
        <wgt-star-rating [value]="fb.rating" [readonly]="true" />
        @if (fb.comment) {
          <p class="comment">{{ fb.comment }}</p>
        }
      </div>
    }
  `,
  styles: [
    `
      .prompt {
        padding: 12px;
        border-top: 1px solid var(--wgt-border, #e0e0e0);
        background: var(--wgt-surface, #fff);
      }
      .heading {
        margin: 0 0 8px;
        font-size: 14px;
        font-weight: 600;
        color: var(--wgt-text, #333);
      }
      .dismiss-btn {
        display: block;
        margin-top: 8px;
        background: none;
        border: none;
        font-size: 12px;
        color: var(--wgt-muted, #999);
        cursor: pointer;
        padding: 4px 0;
      }
      .dismiss-btn:hover {
        color: var(--wgt-text, #333);
      }
      .collapsed-btn {
        display: block;
        width: 100%;
        padding: 8px 12px;
        border: none;
        border-top: 1px solid var(--wgt-border, #e0e0e0);
        background: var(--wgt-surface, #fff);
        font-size: 13px;
        color: var(--wgt-link, #4a90d9);
        cursor: pointer;
        text-align: center;
      }
      .collapsed-btn:hover {
        background: var(--wgt-hover, #f5f5f5);
      }
      .submitted {
        padding: 12px;
        border-top: 1px solid var(--wgt-border, #e0e0e0);
        background: var(--wgt-surface, #fff);
      }
      .thank-you {
        margin: 0 0 8px;
        font-size: 14px;
        font-weight: 600;
        color: var(--wgt-text, #333);
      }
      .comment {
        margin: 8px 0 0;
        font-size: 13px;
        color: var(--wgt-muted, #666);
        font-style: italic;
      }
      .comment-area {
        margin-top: 8px;
      }
      .comment-input {
        width: 100%;
        box-sizing: border-box;
        padding: 8px;
        border: 1px solid var(--wgt-border, #e0e0e0);
        border-radius: 6px;
        font-family: inherit;
        font-size: 13px;
        color: var(--wgt-text, #333);
        background: var(--wgt-surface, #fff);
        resize: vertical;
        min-height: 60px;
        outline: none;
      }
      .comment-input:focus {
        border-color: var(--wgt-link, #4a90d9);
      }
      .char-counter {
        display: block;
        text-align: right;
        font-size: 11px;
        color: var(--wgt-muted, #999);
        margin-top: 2px;
      }
      .char-counter.over {
        color: var(--wgt-error, #e53935);
      }
      .submit-btn {
        display: block;
        width: 100%;
        margin-top: 8px;
        padding: 8px;
        background: var(--wgt-primary, #4a90d9);
        color: #fff;
        border: none;
        border-radius: 6px;
        font-size: 13px;
        cursor: pointer;
      }
      .submit-btn:disabled {
        opacity: 0.5;
        cursor: default;
      }
    `,
  ],
})
export class FeedbackPromptComponent {
  readonly state = input.required<'prompt' | 'collapsed' | 'submitted'>();
  readonly feedback = input<WidgetFeedback | null>(null);
  readonly submitFeedback = output<{ rating: number; comment?: string }>();
  readonly dismiss = output<void>();
  readonly expand = output<void>();

  readonly selectedRating = signal<number>(0);
  readonly comment = signal('');
  readonly charCount = computed(() => this.comment().length);
  readonly overLimit = computed(() => this.charCount() > 2000);

  onRate(star: number): void {
    this.selectedRating.set(star);
  }

  onSubmit(): void {
    this.submitFeedback.emit({
      rating: this.selectedRating(),
      comment: this.comment() || undefined,
    });
    this.selectedRating.set(0);
    this.comment.set('');
  }

  onInput(event: Event): void {
    this.comment.set((event.target as HTMLTextAreaElement).value);
  }
}
