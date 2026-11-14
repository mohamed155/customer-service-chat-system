import { ChangeDetectionStrategy, Component, computed, inject, input, model } from '@angular/core';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { ownValue } from '../../../core/i18n/own-value';
import { ChoiceGroupComponent } from '../../../shared/components/choice-group/choice-group.component';
import { ChoiceGroupOption } from '../../../shared/components/choice-group/choice-group.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

/** Catalog keys of the tones the API offers. A tone this app has no name for is shown as sent. */
const TONE_KEYS: Readonly<Record<string, string>> = {
  professional: marker('ai-agent.tone.professional'),
  friendly: marker('ai-agent.tone.friendly'),
  casual: marker('ai-agent.tone.casual'),
  formal: marker('ai-agent.tone.formal'),
  empathetic: marker('ai-agent.tone.empathetic'),
};

@Component({
  selector: 'app-tone-selector',
  standalone: true,
  providers: [provideTranslocoScope('ai-agent')],
  imports: [ChoiceGroupComponent, TranslocoPipe],
  templateUrl: './tone-selector.component.html',
  styles: [
    `
      :host {
        display: block;
      }
      .tone-selector {
        display: grid;
        gap: var(--app-space-2);
      }
      .label {
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 700;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToneSelectorComponent {
  private readonly locale = inject(LocaleService);

  readonly value = model<string>('');
  readonly tones = input<string[]>([]);

  protected readonly options = computed<ChoiceGroupOption[]>(() =>
    this.tones().map((t) => {
      const key = ownValue(TONE_KEYS, t);
      // A tone with no name in the catalog is shown as sent, with a capital.
      return {
        value: t,
        label: key ? this.locale.translate(key) : t.charAt(0).toUpperCase() + t.slice(1),
      };
    }),
  );
}
