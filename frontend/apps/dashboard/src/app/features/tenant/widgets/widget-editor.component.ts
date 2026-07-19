import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { WidgetInstance } from '../../../core/api/widget.models';
import { ButtonComponent } from '../../../shared/components/button/button.component';

interface ValidationErrors {
  name?: string;
  displayName?: string;
  primaryColor?: string;
  welcomeMessage?: string;
  allowedDomains?: string;
}

@Component({
  selector: 'app-widget-editor',
  standalone: true,
  imports: [ButtonComponent, FormsModule],
  host: { '[class.visible]': 'visible()' },
  template: `
    <div class="editor-panel">
      <div class="editor-header">
        <h3>{{ isNew() ? 'New Widget' : 'Edit Widget' }}</h3>
        <button type="button" class="close-btn" (click)="dismissed.emit()" aria-label="Close">
          &times;
        </button>
      </div>

      @if (visible()) {
        <div class="editor-body">
          <div class="field">
            <label for="wgt-name">Name *</label>
            <input
              id="wgt-name"
              type="text"
              placeholder="My Widget"
              [ngModel]="form().name"
              (ngModelChange)="onChange('name', $event)"
            />
            @if (errors().name) {
              <span class="field-error">{{ errors().name }}</span>
            }
          </div>

          <div class="field">
            <label for="wgt-display-name">Display name</label>
            <input
              id="wgt-display-name"
              type="text"
              placeholder="Support Chat"
              [ngModel]="form().displayName"
              (ngModelChange)="onChange('displayName', $event)"
            />
          </div>

          <div class="field">
            <label for="wgt-primary-color">Primary color</label>
            <div class="color-row">
              <input
                id="wgt-primary-color"
                type="color"
                [ngModel]="form().primaryColor || '#0066FF'"
                (ngModelChange)="onChange('primaryColor', $event)"
              />
              <input
                type="text"
                class="color-hex"
                placeholder="#0066FF"
                maxlength="7"
                [ngModel]="form().primaryColor"
                (ngModelChange)="onChange('primaryColor', $event)"
              />
            </div>
            @if (errors().primaryColor) {
              <span class="field-error">{{ errors().primaryColor }}</span>
            }
          </div>

          <div class="field">
            <label for="wgt-welcome">Welcome message</label>
            <textarea
              id="wgt-welcome"
              rows="3"
              placeholder="Hello! How can we help you today?"
              [ngModel]="form().welcomeMessage"
              (ngModelChange)="onChange('welcomeMessage', $event)"
            ></textarea>
            @if (errors().welcomeMessage) {
              <span class="field-error">{{ errors().welcomeMessage }}</span>
            }
          </div>

          <div class="field-row">
            <div class="field">
              <label for="wgt-position">Position</label>
              <select
                id="wgt-position"
                [ngModel]="form().position || 'bottom-right'"
                (ngModelChange)="onChange('position', $event)"
              >
                <option value="bottom-right">Bottom Right</option>
                <option value="bottom-left">Bottom Left</option>
              </select>
            </div>
            <div class="field">
              <label for="wgt-theme">Theme</label>
              <select
                id="wgt-theme"
                [ngModel]="form().theme || 'light'"
                (ngModelChange)="onChange('theme', $event)"
              >
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </select>
            </div>
          </div>

          <div class="field toggle-row">
            <label for="wgt-enabled">Enabled</label>
            <input
              id="wgt-enabled"
              type="checkbox"
              [checked]="form().enabled !== false"
              (change)="toggleEnabled($event)"
            />
          </div>

          <div class="field">
            <span class="field-label">Allowed domains</span>
            <div class="domains-list">
              @for (domain of form().allowedDomains || []; track $index) {
                <div class="domain-chip">
                  <span>{{ domain }}</span>
                  <button
                    type="button"
                    class="domain-remove"
                    (click)="removeDomain($index)"
                    aria-label="Remove domain"
                  >
                    &times;
                  </button>
                </div>
              }
            </div>
            <div class="domain-add-row">
              <input
                type="text"
                class="domain-input"
                placeholder="example.com"
                [ngModel]="newDomain()"
                (ngModelChange)="newDomain.set($event)"
                (keydown.enter)="addDomain()"
              />
              <button
                type="button"
                class="domain-add-btn"
                (click)="addDomain()"
                [disabled]="!newDomain().trim()"
              >
                Add
              </button>
            </div>
            @if (errors().allowedDomains) {
              <span class="field-error">{{ errors().allowedDomains }}</span>
            }
          </div>

          @if (error()) {
            <div class="error-banner">{{ error() }}</div>
          }
        </div>

        <div class="editor-footer">
          <app-button variant="secondary" size="sm" (pressed)="dismissed.emit()">
            Cancel
          </app-button>
          <app-button
            variant="primary"
            size="sm"
            [disabled]="!isValid() || saving()"
            (pressed)="save.emit()"
          >
            {{ saving() ? 'Saving…' : isNew() ? 'Create' : 'Save' }}
          </app-button>
        </div>
      }
    </div>
  `,
  styles: [
    `
      .editor-panel {
        display: none;
        flex-direction: column;
        background: var(--app-panel);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-xl);
        overflow: hidden;
      }
      .editor-panel.visible {
        display: flex;
      }
      .editor-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: var(--app-space-4);
        border-bottom: 1px solid var(--app-border);
      }
      h3 {
        margin: 0;
        color: var(--app-text);
        font-size: var(--app-font-base);
      }
      .close-btn {
        width: 32px;
        height: 32px;
        border: none;
        background: none;
        color: var(--app-text-2);
        font-size: 20px;
        cursor: pointer;
        border-radius: var(--app-radius-md);
        display: grid;
        place-items: center;
      }
      .close-btn:hover {
        background: var(--app-panel-2);
      }
      .editor-body {
        padding: var(--app-space-4);
        display: grid;
        gap: var(--app-space-4);
      }
      .field {
        display: grid;
        gap: 6px;
      }
      .field label,
      .field-label {
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
      }
      .field input:not([type='color']):not([type='checkbox']),
      .field textarea,
      .field select {
        height: 38px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        font-size: var(--app-font-sm);
        outline: none;
      }
      .field textarea {
        height: auto;
        padding: var(--app-space-2) var(--app-space-3);
        resize: vertical;
        min-height: 60px;
      }
      .field input:focus,
      .field textarea:focus,
      .field select:focus {
        border-color: var(--app-accent);
      }
      .field-row {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: var(--app-space-4);
      }
      .color-row {
        display: flex;
        gap: var(--app-space-2);
        align-items: center;
      }
      .color-row input[type='color'] {
        width: 42px;
        height: 38px;
        padding: 2px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        cursor: pointer;
        background: none;
      }
      .color-hex {
        flex: 1;
      }
      .toggle-row {
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
      }
      .toggle-row input[type='checkbox'] {
        width: 18px;
        height: 18px;
        cursor: pointer;
      }
      .field-error {
        color: var(--app-red, #e53935);
        font-size: var(--app-font-xs);
      }
      .error-banner {
        padding: var(--app-space-3);
        background: rgba(229, 57, 53, 0.1);
        border: 1px solid var(--app-red, #e53935);
        border-radius: var(--app-radius-md);
        color: var(--app-red, #e53935);
        font-size: var(--app-font-sm);
      }
      .domains-list {
        display: flex;
        flex-wrap: wrap;
        gap: 6px;
      }
      .domain-chip {
        display: inline-flex;
        align-items: center;
        gap: 4px;
        padding: 4px 8px;
        border-radius: 999px;
        background: var(--app-panel-2);
        border: 1px solid var(--app-border);
        font-size: var(--app-font-xs);
        color: var(--app-text);
      }
      .domain-remove {
        border: none;
        background: none;
        color: var(--app-text-3);
        cursor: pointer;
        font-size: 14px;
        line-height: 1;
        padding: 0;
      }
      .domain-remove:hover {
        color: var(--app-red, #e53935);
      }
      .domain-add-row {
        display: flex;
        gap: var(--app-space-2);
      }
      .domain-input {
        flex: 1;
      }
      .domain-add-btn {
        height: 38px;
        padding: 0 var(--app-space-4);
        border: 1px solid var(--app-accent);
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 650;
        font-size: var(--app-font-sm);
        cursor: pointer;
      }
      .domain-add-btn:disabled {
        opacity: 0.5;
        cursor: default;
      }
      .editor-footer {
        display: flex;
        justify-content: flex-end;
        gap: var(--app-space-2);
        padding: var(--app-space-4);
        border-top: 1px solid var(--app-border);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class WidgetEditorComponent {
  readonly form = input.required<Partial<WidgetInstance>>();
  readonly isNew = input(false);
  readonly visible = input(true);
  readonly saving = input(false);
  readonly error = input<string | null>(null);

  readonly save = output<void>();
  readonly dismissed = output<void>();
  readonly formChange = output<Partial<WidgetInstance>>();

  protected readonly newDomain = signal('');

  private readonly hexColorRe = /^#[0-9a-fA-F]{6}$/;

  protected readonly errors = computed<ValidationErrors>(() => {
    const f = this.form();
    const errs: ValidationErrors = {};

    const name = (f.name ?? '').trim();
    if (!name || name.length < 1) errs.name = 'Name is required';
    else if (name.length > 80) errs.name = 'Name must be 80 characters or fewer';

    const displayName = (f.displayName ?? '').trim();
    if (displayName.length > 80) errs.displayName = 'Display name must be 80 characters or fewer';

    const color = f.primaryColor || '';
    if (color && !this.hexColorRe.test(color))
      errs.primaryColor = 'Must be a valid hex color (e.g. #0066FF)';

    const msg = f.welcomeMessage || '';
    if (msg.length > 500) errs.welcomeMessage = 'Message must be 500 characters or fewer';

    const domains = f.allowedDomains || [];
    if (domains.length > 20) errs.allowedDomains = 'Maximum 20 domains allowed';
    const invalidDomain = domains.find(
      (d) => !/^(?:\*\.)?[a-zA-Z0-9][a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$/.test(d),
    );
    if (invalidDomain) errs.allowedDomains = `Invalid domain: ${invalidDomain}`;

    return errs;
  });

  protected readonly isValid = computed(() => {
    const errs = this.errors();
    const f = this.form();
    return Object.keys(errs).length === 0 && (f.name ?? '').trim().length > 0;
  });

  protected onChange(field: string, value: unknown): void {
    this.formChange.emit({ [field]: value });
  }

  protected toggleEnabled(event: Event): void {
    const checked = (event.target as HTMLInputElement).checked;
    this.onChange('enabled', checked);
  }

  protected addDomain(): void {
    const domain = this.newDomain().trim();
    if (!domain) return;
    const current = [...(this.form().allowedDomains || [])];
    if (!current.includes(domain)) {
      current.push(domain);
      this.onChange('allowedDomains', current);
    }
    this.newDomain.set('');
  }

  protected removeDomain(index: number): void {
    const current = [...(this.form().allowedDomains || [])];
    current.splice(index, 1);
    this.onChange('allowedDomains', current);
  }
}
