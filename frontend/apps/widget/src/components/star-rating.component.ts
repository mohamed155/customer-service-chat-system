import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

@Component({
  standalone: true,
  selector: 'wgt-star-rating',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div
      role="radiogroup"
      class="stars"
      [attr.aria-label]="'Rating: ' + value() + ' out of 5'"
    >
      @for (star of [1, 2, 3, 4, 5]; track star) {
        <button
          type="button"
          class="star"
          [class.filled]="star <= value()"
          [class.readonly]="readonly()"
          [attr.aria-label]="star + ' stars'"
          [attr.aria-checked]="star === value()"
          [disabled]="readonly()"
          (click)="onRate(star)"
        >
          {{ star <= value() ? '\u2605' : '\u2606' }}
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
  readonly value = input<number>(0);
  readonly readonly = input<boolean>(false);
  readonly rate = output<number>();

  onRate(star: number): void {
    if (!this.readonly()) {
      this.rate.emit(star);
    }
  }
}
