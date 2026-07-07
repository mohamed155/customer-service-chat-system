import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

@Component({
  selector: 'app-ai-confidence-badge',
  host: { '[class.low]': 'confidence() < 0.75' },
  template: `AI {{ percent() }}%`,
  styles: [
    `
      :host {
        display: inline-flex;
        align-items: center;
        height: 22px;
        padding: 0 var(--app-space-2);
        border-radius: 999px;
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
        font-size: var(--app-font-xs);
        font-weight: 650;
      }
      :host(.low) {
        background: var(--app-amber-soft);
        color: var(--app-amber);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AiConfidenceBadgeComponent {
  readonly confidence = input.required<number>();
  protected readonly percent = computed(() => Math.round(this.confidence() * 100));
}
