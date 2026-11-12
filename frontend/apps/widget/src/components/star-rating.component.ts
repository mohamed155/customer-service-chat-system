import { ChangeDetectionStrategy, Component, input, output, inject } from '@angular/core';

import { I18n } from '../i18n/i18n.service';

@Component({
  standalone: true,
  selector: 'wgt-star-rating',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div
      role="radiogroup"
      class="stars"
      [attr.aria-label]="i18n.t('rating.group', { value: value() })"
    >
      @for (star of [1, 2, 3, 4, 5]; track star) {
        <button
          type="button"
          class="star"
          [class.filled]="star <= value()"
          [class.readonly]="readonly()"
          [attr.aria-label]="i18n.t('rating.star', { count: star })"
          [attr.aria-checked]="star === value()"
          [disabled]="readonly()"
          (click)="onRate(star)"
        >
          {{ star <= value() ? '★' : '☆' }}
        </button>
      }
    </div>
  `,
  styles: [
    `
      .stars {
        display: flex;
        gap: 4px;
      }
      .star {
        background: none;
        border: none;
        font-size: 24px;
        cursor: pointer;
        color: var(--wgt-star-empty, #ccc);
        padding: 2px;
        line-height: 1;
        transition: color 0.15s;
      }
      .star.filled {
        color: var(--wgt-star-filled, #f5a623);
      }
      .star.readonly {
        cursor: default;
      }
      .star:not(.readonly):hover {
        color: var(--wgt-star-hover, #f5a623);
      }
      .star:focus-visible {
        outline: 2px solid var(--wgt-focus, #4a90d9);
        outline-offset: 2px;
        border-radius: 2px;
      }
    `,
  ],
})
export class StarRatingComponent {
  protected readonly i18n = inject(I18n);
  readonly value = input<number>(0);
  readonly readonly = input<boolean>(false);
  readonly rate = output<number>();

  onRate(star: number): void {
    if (!this.readonly()) {
      this.rate.emit(star);
    }
  }
}
