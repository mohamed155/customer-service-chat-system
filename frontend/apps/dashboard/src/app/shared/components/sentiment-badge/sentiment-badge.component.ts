import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { Sentiment } from '../../fixtures/fixture.models';

@Component({
  selector: 'app-sentiment-badge',
  host: { '[class]': 'sentiment()' },
  template: `<span>{{ label() }}</span>`,
  styles: [
    `
      :host {
        display: inline-flex;
        align-items: center;
        min-height: 22px;
        padding: 0 var(--app-space-2);
        border-radius: 999px;
        font-size: var(--app-font-xs);
        font-weight: 600;
        white-space: nowrap;
      }
      :host(.positive) {
        background: var(--app-green-soft);
        color: var(--app-green);
      }
      :host(.neutral) {
        background: var(--app-panel-2);
        color: var(--app-text-2);
      }
      :host(.angry) {
        background: var(--app-red-soft);
        color: var(--app-red);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SentimentBadgeComponent {
  readonly sentiment = input.required<Sentiment>();
  protected readonly label = computed(() =>
    this.sentiment().replace(/\b\w/g, (character) => character.toUpperCase()),
  );
}
