import { ChangeDetectionStrategy, Component, computed, input, model } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { TuiIcon } from '@taiga-ui/core';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';

@Component({
  selector: 'app-prompt-editor',
  standalone: true,
  imports: [FormsModule, FormFieldComponent, TuiIcon],
  template: `
    <app-form-field label="System Prompt">
      <textarea
        [ngModel]="value()"
        (ngModelChange)="value.set($event)"
        [maxLength]="maxLength()"
        rows="8"
      ></textarea>
      <div class="counter" [class.warning]="nearLimit()">
        @if (nearLimit()) {
          <tui-icon icon="@tui.alert-triangle" />
        }
        {{ value().length }} / {{ maxLength() }}
      </div>
    </app-form-field>
  `,
  styles: [
    `
      :host {
        display: block;
      }
      textarea {
        width: 100%;
        box-sizing: border-box;
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font: inherit;
        font-size: var(--app-font-sm);
        line-height: 1.6;
        resize: vertical;
      }
      textarea:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-color: var(--app-accent);
      }
      .counter {
        display: flex;
        align-items: center;
        gap: var(--app-space-1);
        margin-top: var(--app-space-1);
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
      }
      .counter.warning {
        color: var(--app-red, #dc2626);
      }
      .counter tui-icon {
        font-size: 14px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PromptEditorComponent {
  readonly value = model('');
  readonly maxLength = input(8000);
  protected readonly nearLimit = computed(() => this.value().length >= this.maxLength() * 0.9);
}
