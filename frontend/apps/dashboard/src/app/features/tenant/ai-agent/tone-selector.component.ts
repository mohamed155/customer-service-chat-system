import { ChangeDetectionStrategy, Component, computed, input, model } from '@angular/core';
import { ChoiceGroupComponent } from '../../../shared/components/choice-group/choice-group.component';
import { ChoiceGroupOption } from '../../../shared/components/choice-group/choice-group.component';

@Component({
  selector: 'app-tone-selector',
  standalone: true,
  imports: [ChoiceGroupComponent],
  template: `
    <div class="tone-selector">
      <span class="label">Response Tone</span>
      <app-choice-group [options]="options()" [(value)]="value" ariaLabel="Response tone" />
    </div>
  `,
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
  readonly value = model<string>('');
  readonly tones = input<string[]>([]);

  protected readonly options = computed<ChoiceGroupOption[]>(() =>
    this.tones().map((t) => ({
      value: t,
      label: t.charAt(0).toUpperCase() + t.slice(1),
    })),
  );
}
