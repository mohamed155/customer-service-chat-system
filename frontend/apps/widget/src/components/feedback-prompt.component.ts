import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
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
        <wgt-star-rating (rate)="submitRating.emit($event)" />
        <!-- comment box added by T023 -->
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
    `,
  ],
})
export class FeedbackPromptComponent {
  readonly state = input.required<'prompt' | 'collapsed' | 'submitted'>();
  readonly feedback = input<WidgetFeedback | null>(null);
  readonly submitRating = output<number>();
  readonly dismiss = output<void>();
  readonly expand = output<void>();
}
