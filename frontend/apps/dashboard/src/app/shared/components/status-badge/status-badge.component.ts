import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

export type BadgeTone = 'green' | 'amber' | 'red' | 'accent' | 'neutral';

@Component({
  selector: 'app-status-badge',
  host: { '[class]': 'toneClass()' },
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
        line-height: 1;
        text-transform: capitalize;
        white-space: nowrap;
      }
      :host(.green) {
        background: var(--app-green-soft);
        color: var(--app-green);
      }
      :host(.amber) {
        background: var(--app-amber-soft);
        color: var(--app-amber);
      }
      :host(.red) {
        background: var(--app-red-soft);
        color: var(--app-red);
      }
      :host(.accent) {
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      :host(.neutral) {
        background: var(--app-panel-2);
        color: var(--app-text-2);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class StatusBadgeComponent {
  readonly status = input.required<string>();
  readonly tone = input<BadgeTone>('neutral');

  protected readonly toneClass = computed(() => this.tone());
  protected readonly label = computed(() =>
    this.status()
      .replaceAll('-', ' ')
      .replace(/\b\w/g, (character) => character.toUpperCase()),
  );
}
