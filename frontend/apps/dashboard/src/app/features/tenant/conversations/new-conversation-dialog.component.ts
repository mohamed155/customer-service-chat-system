import { ChangeDetectionStrategy, Component, inject, output, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { FormBuilder, ReactiveFormsModule, Validators } from '@angular/forms';
import { debounceTime, distinctUntilChanged, of, switchMap } from 'rxjs';
import { ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import { CreateConversationPayload, Customer } from '../../../core/api/tenant-api.models';
import { CustomersApiService } from '../customers/customers-api.service';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DialogShellComponent } from '../../../shared/components/dialog-shell/dialog-shell.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { ConversationsApiService } from './conversations-api.service';

@Component({
  selector: 'app-new-conversation-dialog',
  imports: [
    ButtonComponent,
    DialogShellComponent,
    FormFieldComponent,
    InlineAlertComponent,
    ReactiveFormsModule,
  ],
  template: `
    <app-dialog-shell
      [open]="true"
      ariaLabelledby="new-conv-title"
      [dismissDisabled]="submitting()"
      (dismiss)="closeDialog.emit()"
    >
      <form [formGroup]="form" (ngSubmit)="submit()">
        <h3 id="new-conv-title">New conversation</h3>

        <app-form-field label="Customer">
          <input
            type="text"
            aria-label="Search customers"
            formControlName="customerSearch"
            placeholder="Search customers by name or email…"
            autocomplete="off"
          />
          @if (searchResults().length) {
            <ul class="customer-results" role="listbox">
              @for (customer of searchResults(); track customer.id) {
                <li
                  role="option"
                  [class.selected]="selectedCustomer()?.id === customer.id"
                  [attr.aria-selected]="selectedCustomer()?.id === customer.id"
                  (click)="selectCustomer(customer)"
                  (keydown.enter)="selectCustomer(customer)"
                  tabindex="0"
                >
                  <strong>{{ customer.displayName }}</strong>
                  @if (customer.email) {
                    <span>{{ customer.email }}</span>
                  }
                </li>
              }
            </ul>
          }
          @if (selectedCustomer(); as customer) {
            <div class="selected-customer">
              Selected: <strong>{{ customer.displayName }}</strong>
              @if (customer.email) {
                <span> ({{ customer.email }})</span>
              }
              <button
                type="button"
                class="remove-btn"
                (click)="clearCustomer()"
                aria-label="Remove selected customer"
              >
                &times;
              </button>
            </div>
          }
          @if (form.controls.customerSearch.touched && !selectedCustomer()) {
            <app-inline-alert tone="error">Select a customer from the results.</app-inline-alert>
          }
        </app-form-field>

        <app-form-field label="Channel">
          <select formControlName="channel" aria-label="Channel">
            <option value="">Select a channel</option>
            <option value="email">Email</option>
            <option value="web_chat">Web Chat</option>
            <option value="whatsapp">WhatsApp</option>
            <option value="telegram">Telegram</option>
            <option value="phone">Phone</option>
          </select>
          @if (form.controls.channel.touched && form.controls.channel.invalid) {
            <app-inline-alert tone="error">Channel is required.</app-inline-alert>
          }
        </app-form-field>

        <app-form-field label="First message">
          <textarea
            formControlName="body"
            aria-label="First message"
            placeholder="Write the first message…"
            rows="4"
          ></textarea>
          @if (form.controls.body.touched && form.controls.body.invalid) {
            <app-inline-alert tone="error"
              >Message is required (1–10,000 characters).</app-inline-alert
            >
          }
        </app-form-field>

        @if (error(); as err) {
          <app-inline-alert tone="error">{{ err }}</app-inline-alert>
        }

        <div class="actions">
          <app-button variant="secondary" (pressed)="closeDialog.emit()">Cancel</app-button>
          <app-button variant="primary" type="submit" [disabled]="submitting() || form.invalid">
            {{ submitting() ? 'Creating…' : 'Create conversation' }}
          </app-button>
        </div>
      </form>
    </app-dialog-shell>
  `,
  styles: [
    `
      h3 {
        margin: 0 0 var(--app-space-4);
        font-size: var(--app-font-lg);
      }
      app-form-field {
        margin-bottom: var(--app-space-4);
        display: block;
      }
      textarea {
        width: 100%;
        box-sizing: border-box;
        min-height: 80px;
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
      .customer-results {
        list-style: none;
        margin: var(--app-space-2) 0 0;
        padding: 0;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        max-height: 200px;
        overflow-y: auto;
      }
      .customer-results li {
        padding: var(--app-space-2) var(--app-space-3);
        cursor: pointer;
        display: grid;
        gap: 2px;
      }
      .customer-results li:hover,
      .customer-results li.selected {
        background: var(--app-accent-soft);
      }
      .customer-results li strong {
        color: var(--app-text);
        font-size: var(--app-font-sm);
      }
      .customer-results li span {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .selected-customer {
        margin-top: var(--app-space-2);
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-accent);
        border-radius: var(--app-radius-md);
        background: var(--app-accent-soft);
        color: var(--app-text);
        font-size: var(--app-font-sm);
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
      }
      .remove-btn {
        margin-left: auto;
        border: 0;
        background: transparent;
        color: var(--app-text-2);
        font-size: var(--app-font-lg);
        cursor: pointer;
        padding: 0 4px;
      }
      .actions {
        display: flex;
        gap: var(--app-space-2);
        justify-content: flex-end;
        margin-top: var(--app-space-4);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NewConversationDialogComponent {
  readonly create = output<string>();
  readonly closeDialog = output<void>();

  private readonly fb = inject(FormBuilder);
  private readonly customersApi = inject(CustomersApiService);
  private readonly convApi = inject(ConversationsApiService);

  protected readonly submitting = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly searchResults = signal<Customer[]>([]);
  protected readonly selectedCustomer = signal<Customer | null>(null);

  protected readonly form = this.fb.nonNullable.group({
    customerSearch: ['', Validators.required],
    channel: ['', Validators.required],
    body: ['', [Validators.required, Validators.minLength(1)]],
  });

  constructor() {
    this.form.controls.customerSearch.valueChanges
      .pipe(
        debounceTime(300),
        distinctUntilChanged(),
        takeUntilDestroyed(),
        switchMap((q: string) => {
          if (!q || q.trim().length < 2)
            return of(null as unknown as ApiResponse<PaginatedResponse<Customer>>);
          return this.customersApi.list({ q: q.trim() });
        }),
      )
      .subscribe({
        next: (response) => {
          if (response) {
            this.searchResults.set(response.data.items);
          } else {
            this.searchResults.set([]);
          }
        },
      });
  }

  protected selectCustomer(customer: Customer): void {
    this.selectedCustomer.set(customer);
    this.form.controls.customerSearch.setValue(customer.displayName);
    this.searchResults.set([]);
  }

  protected clearCustomer(): void {
    this.selectedCustomer.set(null);
    this.form.controls.customerSearch.setValue('');
    this.searchResults.set([]);
  }

  protected submit(): void {
    if (this.form.invalid || !this.selectedCustomer()) {
      this.form.markAllAsTouched();
      return;
    }

    const raw = this.form.getRawValue();
    const body = raw.body.trim();
    if (!body) return;

    this.submitting.set(true);
    this.error.set(null);

    const payload: CreateConversationPayload = {
      customerId: this.selectedCustomer()!.id,
      channel: raw.channel,
      message: { body },
    };

    this.convApi.create(payload).subscribe({
      next: (response) => {
        this.create.emit(response.data.id);
      },
      error: (err: unknown) => {
        this.submitting.set(false);
        this.error.set((err as Error)?.message ?? 'Failed to create conversation');
      },
    });
  }
}
