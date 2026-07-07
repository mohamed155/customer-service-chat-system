import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'app-avatar',
  host: { '[class]': 'size()' },
  template: `{{ initials() }}`,
  styles: [
    `
      :host {
        display: inline-grid;
        place-items: center;
        flex: 0 0 auto;
        border-radius: 999px;
        background: linear-gradient(135deg, var(--app-accent), var(--app-accent-strong));
        color: var(--app-accent-ink);
        font-weight: 700;
        letter-spacing: 0;
      }
      :host(.sm) {
        width: 28px;
        height: 28px;
        font-size: var(--app-font-xs);
      }
      :host(.md) {
        width: 36px;
        height: 36px;
        font-size: var(--app-font-sm);
      }
      :host(.lg) {
        width: 46px;
        height: 46px;
        font-size: var(--app-font-base);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AvatarComponent {
  readonly initials = input.required<string>();
  readonly size = input<'sm' | 'md' | 'lg'>('md');
}
