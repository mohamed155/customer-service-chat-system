import { ChangeDetectionStrategy, Component, computed, input, inject } from '@angular/core';
import { marker } from '../../../core/i18n/marker';
import { LocaleService } from '../../../core/i18n/locale.service';

@Component({
  imports: [],
  selector: 'app-satisfaction-badge',
  host: { '[class]': 'toneClass()', '[attr.aria-label]': 'ariaLabel()' },
  templateUrl: './satisfaction-badge.component.html',
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
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SatisfactionBadgeComponent {
  private readonly locale = inject(LocaleService);

  readonly rating = input.required<number>();

  protected readonly toneClass = computed(() => {
    const r = this.rating();
    if (r >= 4) return 'green';
    if (r >= 3) return 'amber';
    return 'red';
  });

  protected readonly ariaLabel = computed(() =>
    this.locale.translate(marker('satisfaction.rated'), { rating: this.rating() }),
  );
}
