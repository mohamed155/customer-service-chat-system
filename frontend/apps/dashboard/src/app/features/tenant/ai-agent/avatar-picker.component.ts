import { ChangeDetectionStrategy, Component, input, model, signal } from '@angular/core';
import { marker } from '../../../core/i18n/marker';
import { ownValue } from '../../../core/i18n/own-value';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

export type AvatarValue = { kind: 'preset'; preset: string } | { kind: 'upload' } | null;

const PRESET_KEYS: Readonly<Record<string, string>> = {
  spark: marker('ai-agent.avatar.presets.spark'),
  orbit: marker('ai-agent.avatar.presets.orbit'),
  beacon: marker('ai-agent.avatar.presets.beacon'),
  nova: marker('ai-agent.avatar.presets.nova'),
  pulse: marker('ai-agent.avatar.presets.pulse'),
  atlas: marker('ai-agent.avatar.presets.atlas'),
};

/** The largest image the avatar may be, in KB. */
const MAX_AVATAR_KB = 256;

@Component({
  selector: 'app-avatar-picker',
  standalone: true,
  providers: [provideTranslocoScope('ai-agent')],
  imports: [ButtonComponent, InlineAlertComponent, TranslocoPipe],
  templateUrl: './avatar-picker.component.html',
  styles: [
    `
      :host {
        display: block;
      }
      .avatar-picker {
        display: grid;
        gap: var(--app-space-3);
      }
      .label {
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 700;
      }
      .preset-grid {
        display: flex;
        flex-wrap: wrap;
        gap: var(--app-space-2);
      }
      .preset-btn {
        width: 56px;
        height: 56px;
        border: 2px solid var(--app-border);
        border-radius: var(--app-radius-lg);
        background: var(--app-panel);
        color: var(--app-text-2);
        cursor: pointer;
        font-size: var(--app-font-xs);
        font-weight: 600;
      }
      .preset-btn:hover {
        border-color: var(--app-accent);
      }
      .preset-btn.selected {
        border-color: var(--app-accent);
        background: var(--app-accent-soft);
        color: var(--app-accent);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AvatarPickerComponent {
  readonly presets = input<string[]>([]);
  readonly value = model<AvatarValue>(null);
  readonly uploadError = signal<{ key: string; params?: Record<string, unknown> } | null>(null);

  /** The catalog key of a preset's name; a preset this app has no name for shows as sent. */
  protected presetKey(preset: string): string | null {
    return ownValue(PRESET_KEYS, preset) ?? null;
  }

  protected isPresetSelected(preset: string): boolean {
    const v = this.value();
    return v?.kind === 'preset' && v.preset === preset;
  }

  protected handleFile(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    if (file.size > MAX_AVATAR_KB * 1024) {
      this.uploadError.set({
        key: marker('ai-agent.avatar.tooLarge'),
        params: { max: MAX_AVATAR_KB },
      });
      input.value = '';
      return;
    }

    this.uploadError.set(null);
    this.value.set({ kind: 'upload' });
  }
}
