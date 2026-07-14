import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';
import { FormBuilder, ReactiveFormsModule, Validators } from '@angular/forms';
import {
  AddMessagePayload,
  ConversationStatus,
  MessageKind,
} from '../../../core/api/tenant-api.models';
import { ButtonComponent } from '../../../shared/components/button/button.component';

@Component({
  selector: 'app-composer',
  imports: [ButtonComponent, ReactiveFormsModule],
  template: `
    <footer class="composer">
      <div class="mode-tabs">
        <button
          type="button"
          class="mode-tab"
          [class.active]="mode() === 'reply'"
          (click)="mode.set('reply')"
        >
          Reply
        </button>
        <button
          type="button"
          class="mode-tab"
          [class.active]="mode() === 'note'"
          (click)="mode.set('note')"
        >
          Internal note
        </button>
        <button
          type="button"
          class="mode-tab"
          [class.active]="mode() === 'customer'"
          (click)="mode.set('customer')"
        >
          Customer
        </button>
      </div>

      <form [formGroup]="form" (ngSubmit)="submit()" class="composer-form">
        <textarea
          formControlName="body"
          aria-label="Message body"
          [placeholder]="
            mode() === 'reply'
              ? 'Type a reply…'
              : mode() === 'note'
                ? 'Add an internal note…'
                : 'Log a customer message…'
          "
          rows="3"
        ></textarea>
        @if (form.controls.body.touched && form.controls.body.invalid) {
          <span class="field-error">Message is required.</span>
        }
        <div class="composer-actions">
          <app-button
            variant="primary"
            type="submit"
            [disabled]="submitting() || !form.controls.body.value.trim()"
          >
            {{ submitting() ? 'Sending…' : 'Send' }}
          </app-button>
        </div>
      </form>
    </footer>
  `,
  styles: [
    `
      .composer {
        border-top: 1px solid var(--app-border);
        background: var(--app-panel);
      }
      .mode-tabs {
        display: flex;
        gap: 0;
        border-bottom: 1px solid var(--app-border);
      }
      .mode-tab {
        height: 34px;
        padding: 0 var(--app-space-4);
        border: 0;
        border-bottom: 2px solid transparent;
        background: transparent;
        color: var(--app-text-2);
        font-weight: 600;
        font-size: var(--app-font-sm);
        cursor: pointer;
      }
      .mode-tab.active {
        color: var(--app-accent-strong);
        border-bottom-color: var(--app-accent);
        background: var(--app-accent-soft);
      }
      .composer-form {
        display: grid;
        gap: var(--app-space-3);
        padding: var(--app-space-3) var(--app-space-4);
      }
      textarea {
        width: 100%;
        box-sizing: border-box;
        min-height: 64px;
        max-height: 200px;
        resize: vertical;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text);
        padding: var(--app-space-3);
        font: inherit;
        font-size: var(--app-font-sm);
      }
      textarea:focus {
        outline: none;
        border-color: var(--app-accent);
      }
      .field-error {
        color: var(--app-red);
        font-size: var(--app-font-xs);
        font-weight: 600;
      }
      .composer-actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--app-space-2);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ComposerComponent {
  readonly conversationId = input.required<string>();
  readonly currentStatus = input.required<ConversationStatus>();
  readonly submitting = input(false);
  readonly send = output<AddMessagePayload>();

  private readonly fb = inject(FormBuilder);

  protected readonly mode = signal<'reply' | 'note' | 'customer'>('reply');

  protected readonly form = this.fb.nonNullable.group({
    body: ['', [Validators.required, Validators.minLength(1)]],
  });

  protected submit(): void {
    if (this.form.invalid) {
      this.form.markAllAsTouched();
      return;
    }

    const body = this.form.controls.body.value.trim();
    if (!body) return;

    this.send.emit({ kind: this.mode() as MessageKind, body });
    this.form.reset();
    this.mode.set('reply');
  }
}
