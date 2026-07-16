import { ChangeDetectionStrategy, Component, computed, input, model } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';

export interface ProviderOption {
  id: string;
  name: string;
  credentialAvailable: boolean;
  models: string[];
}

export interface ProviderModelValue {
  providerId: string | null;
  model: string | null;
}

@Component({
  selector: 'app-provider-model-selector',
  standalone: true,
  imports: [FormsModule, InlineAlertComponent],
  template: `
    <div class="selector">
      <label class="label" for="ai-provider-select">AI Provider</label>
      <select
        id="ai-provider-select"
        [ngModel]="value().providerId"
        (ngModelChange)="setProvider($event)"
      >
        <option [ngValue]="null">Follow platform default</option>
        @for (provider of availableProviders(); track provider.id) {
          <option [ngValue]="provider.id">{{ provider.name }}</option>
        }
      </select>

      @if (value().providerId) {
        <label class="label" for="ai-model-select">Model</label>
        <select id="ai-model-select" [ngModel]="value().model" (ngModelChange)="setModel($event)">
          <option [ngValue]="null">Follow platform default</option>
          @for (model of selectedModels(); track model) {
            <option [ngValue]="model">{{ model }}</option>
          }
        </select>
      }

      @if (stale()) {
        <app-inline-alert tone="info">
          This provider selection may be stale — the credential status may have changed.
        </app-inline-alert>
      }
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
      }
      .selector {
        display: grid;
        gap: var(--app-space-3);
      }
      .label {
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 700;
      }
      select {
        width: 100%;
        height: 38px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font: inherit;
      }
      select:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-color: var(--app-accent);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ProviderModelSelectorComponent {
  readonly providers = input<ProviderOption[]>([]);
  readonly value = model<ProviderModelValue>({ providerId: null, model: null });
  readonly stale = input(false);

  protected readonly availableProviders = computed(() =>
    this.providers().filter((p) => p.credentialAvailable),
  );

  protected readonly selectedModels = computed(() => {
    const provider = this.providers().find((p) => p.id === this.value().providerId);
    return provider?.models ?? [];
  });

  protected setProvider(providerId: string | null): void {
    this.value.set({ providerId, model: null });
  }

  protected setModel(model: string | null): void {
    this.value.set({ ...this.value(), model });
  }
}
