import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

@Component({
  selector: 'app-satisfaction-summary',
  standalone: true,
  templateUrl: './satisfaction-summary.component.html',
  styles: [
    `
      .summary-card {
        display: flex;
        align-items: baseline;
        gap: var(--app-space-2);
        padding: var(--app-space-3) var(--app-space-4);
        margin-bottom: var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
      }
      .summary-card.empty {
        justify-content: center;
      }
      .average {
        font-size: var(--app-font-2xl);
        font-weight: 700;
        color: var(--app-text);
      }
      .caption {
        font-size: var(--app-font-sm);
        color: var(--app-text-2);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SatisfactionSummaryComponent {
  readonly averageRating = input<number | null>(null);
  readonly feedbackCount = input<number>(0);

  protected readonly averageDisplay = computed(() => {
    const r = this.averageRating();
    if (r === null) return '—';
    return r.toFixed(1);
  });
}
