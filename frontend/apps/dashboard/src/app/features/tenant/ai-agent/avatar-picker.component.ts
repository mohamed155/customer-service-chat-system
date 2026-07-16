import { ChangeDetectionStrategy, Component, input, model, signal } from '@angular/core';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';

export type AvatarValue = { kind: 'preset'; preset: string } | { kind: 'upload' } | null;

@Component({
  selector: 'app-avatar-picker',
  standalone: true,
  imports: [ButtonComponent, InlineAlertComponent],
  template: `
    <div class="avatar-picker">
      <span class="label">Avatar</span>

      <div class="preset-grid">
        @for (preset of presets(); track preset) {
          <button
            type="button"
            class="preset-btn"
            [class.selected]="isPresetSelected(preset)"
            (click)="value.set({ kind: 'preset', preset })"
          >
            {{ preset }}
          </button>
        }
      </div>

      <app-button variant="secondary" (pressed)="fileInput.click()"> Upload Image </app-button>
      <input
        #fileInput
        type="file"
        accept="image/png,image/jpeg,image/webp"
        (change)="handleFile($event)"
        hidden
      />

      @if (uploadError(); as err) {
        <app-inline-alert tone="error">{{ err }}</app-inline-alert>
      }
    </div>
  `,
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
  readonly uploadError = signal<string | null>(null);

  protected isPresetSelected(preset: string): boolean {
    const v = this.value();
    return v?.kind === 'preset' && v.preset === preset;
  }

  protected handleFile(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    if (file.size > 256 * 1024) {
      this.uploadError.set('Image must be smaller than 256 KB');
      input.value = '';
      return;
    }

    this.uploadError.set(null);
    this.value.set({ kind: 'upload' });
  }
}
