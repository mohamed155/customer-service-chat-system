import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import {
  AbstractControl,
  FormArray,
  FormBuilder,
  FormGroup,
  ReactiveFormsModule,
  ValidationErrors,
  Validators,
} from '@angular/forms';
import {
  ChannelIdentifier,
  CreateCustomerPayload,
  CustomerDetail,
  UpdateCustomerPayload,
} from '../../../core/api/tenant-api.models';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DialogShellComponent } from '../../../shared/components/dialog-shell/dialog-shell.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { IconButtonComponent } from '../../../shared/components/icon-button/icon-button.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { ApiError } from '../../../core/api/api.models';

export type CustomerDialogMode = 'create' | 'edit';

const CHANNELS = ['email', 'phone', 'web_chat', 'whatsapp', 'telegram'] as const;

function phoneValidator(control: AbstractControl): ValidationErrors | null {
  const value = control.value;
  if (!value) return null;
  const str = String(value);
  const digits = str.replace(/\D/g, '');
  if (digits.length < 7 || digits.length > 15)
    return { phone: 'Phone number must have 7-15 digits (optional + prefix).' };
  return null;
}

function channelIdentifierValidator(control: AbstractControl): ValidationErrors | null {
  const group = control as FormGroup;
  const channel = group.get('channel')?.value as string | null;
  const identifier = group.get('identifier')?.value as string | null;
  if (!channel || !identifier) return null;

  if (!identifier.trim()) {
    return { channelIdentifier: 'Identifier value is required.' };
  }

  if (channel === 'email') {
    const parts = identifier.split('@');
    if (parts.length !== 2 || !parts[0] || !parts[1]) {
      return { channelIdentifier: 'Email channel identifiers must be a valid email address.' };
    }
    const local = parts[0];
    const domain = parts[1];
    if (local.length > 64) {
      return { channelIdentifier: 'Email local part must be at most 64 characters.' };
    }
    const labels = domain.split('.');
    if (labels.length < 2 || labels.some((l) => !l || l.length > 63)) {
      return { channelIdentifier: 'Email channel identifiers must be a valid email address.' };
    }
    const tld = labels[labels.length - 1];
    if (tld.length < 2) {
      return { channelIdentifier: 'Email channel identifiers must be a valid email address.' };
    }
  }

  if (channel === 'phone' || channel === 'whatsapp') {
    const str = String(identifier);
    const digits = str.replace(/\D/g, '');
    if (digits.length < 7 || digits.length > 15) {
      return {
        channelIdentifier: `${CHANNEL_LABELS[channel] ?? channel} channel must be a valid phone number (7-15 digits, optional + prefix).`,
      };
    }
  }
  return null;
}

function contactOrIdentifierValidator(control: AbstractControl): ValidationErrors | null {
  const group = control as FormGroup;
  const email = group.get('email')?.value as string | null;
  const phone = group.get('phone')?.value as string | null;
  const identifiers = group.get('identifiers') as FormArray | null;
  if (email || phone) return null;
  if (identifiers && identifiers.length > 0) {
    const hasValid = identifiers.controls.some((ctrl) => {
      const val = (ctrl as FormGroup).get('identifier')?.value as string | null;
      return !!val?.trim();
    });
    if (hasValid) return null;
  }
  return {
    contactOrIdentifier: 'At least one contact method (email, phone, or identifier) is required.',
  };
}

const CHANNEL_LABELS: Record<string, string> = {
  email: 'Email',
  phone: 'Phone',
  web_chat: 'Web chat',
  whatsapp: 'WhatsApp',
  telegram: 'Telegram',
};

@Component({
  selector: 'app-customer-dialog',
  imports: [
    ButtonComponent,
    DialogShellComponent,
    FormFieldComponent,
    IconButtonComponent,
    InlineAlertComponent,
    ReactiveFormsModule,
  ],
  template: `
    <app-dialog-shell
      [open]="true"
      ariaLabelledby="customer-dialog-title"
      [dismissDisabled]="submitting()"
      (dismiss)="closeDialog.emit()"
    >
      <form [formGroup]="form" (ngSubmit)="submit()">
        <h3 id="customer-dialog-title">
          {{ mode() === 'create' ? 'New customer' : 'Edit customer' }}
        </h3>

        <app-form-field label="Display name">
          <input
            type="text"
            aria-label="Display name"
            formControlName="displayName"
            placeholder="Full name"
            maxlength="200"
          />
        </app-form-field>
        @if (form.controls.displayName.touched && form.controls.displayName.invalid) {
          <app-inline-alert tone="error">
            Display name is required (1–200 characters).
          </app-inline-alert>
        }
        @if (fieldError('displayName'); as err) {
          <app-inline-alert tone="error">{{ err }}</app-inline-alert>
        }

        <app-form-field label="Email">
          <input
            type="email"
            aria-label="Email"
            formControlName="email"
            placeholder="Email address"
            maxlength="320"
          />
        </app-form-field>
        @if (form.controls.email.touched && form.controls.email.invalid) {
          <app-inline-alert tone="error"
            >Enter a valid email address (max 320 characters).</app-inline-alert
          >
        }
        @if (fieldError('email'); as err) {
          <app-inline-alert tone="error">{{ err }}</app-inline-alert>
        }

        <app-form-field label="Phone">
          <input
            type="tel"
            aria-label="Phone"
            formControlName="phone"
            placeholder="+201001234567"
            maxlength="16"
          />
        </app-form-field>
        @if (form.controls.phone.touched && form.controls.phone.invalid) {
          <app-inline-alert tone="error"
            >Enter a valid phone number (7–15 digits, optional + prefix).</app-inline-alert
          >
        }
        @if (fieldError('phone'); as err) {
          <app-inline-alert tone="error">{{ err }}</app-inline-alert>
        }

        <fieldset class="identifiers-section">
          <legend>Channel identifiers</legend>
          <div formArrayName="identifiers">
            @for (row of identifierRows(); track row.index) {
              <div class="identifier-row" [formGroupName]="row.index">
                <app-form-field label="Channel">
                  <select formControlName="channel" aria-label="Channel">
                    @for (ch of channels; track ch) {
                      <option [value]="ch">{{ channelLabel(ch) }}</option>
                    }
                  </select>
                  @if (identifierError(row.index, 'channel'); as err) {
                    <app-inline-alert tone="error">{{ err }}</app-inline-alert>
                  }
                </app-form-field>
                <app-form-field label="Identifier">
                  <input
                    type="text"
                    formControlName="identifier"
                    aria-label="Identifier"
                    placeholder="Value"
                    maxlength="320"
                  />
                  @if (identifierError(row.index, 'identifier'); as err) {
                    <app-inline-alert tone="error">{{ err }}</app-inline-alert>
                  }
                </app-form-field>
                <app-icon-button
                  icon="@tui.x"
                  label="Remove identifier"
                  (click)="removeIdentifier(row.index)"
                />
              </div>
            }
          </div>
          <app-button variant="secondary" size="sm" (pressed)="addIdentifier()">
            + Add identifier
          </app-button>
        </fieldset>

        <fieldset class="metadata-section">
          <legend>Metadata ({{ metadataRows().length }}/50)</legend>
          <div formArrayName="metadata">
            @for (row of metadataRows(); track row.index) {
              <div class="metadata-row" [formGroupName]="row.index">
                <app-form-field label="Key">
                  <input
                    type="text"
                    formControlName="key"
                    aria-label="Metadata key"
                    placeholder="Key"
                    maxlength="100"
                  />
                </app-form-field>
                <app-form-field label="Value">
                  <input
                    type="text"
                    formControlName="value"
                    aria-label="Metadata value"
                    placeholder="Value"
                    maxlength="500"
                  />
                  @if (metadataError(row.index); as err) {
                    <app-inline-alert tone="error">{{ err }}</app-inline-alert>
                  }
                </app-form-field>
                <app-icon-button
                  icon="@tui.x"
                  label="Remove metadata"
                  (click)="removeMetadata(row.index)"
                />
              </div>
            }
          </div>
          <app-button variant="secondary" size="sm" (pressed)="addMetadata()"
            >+ Add metadata</app-button
          >
          @if (metadataApproachingLimit()) {
            <app-inline-alert tone="info"
              >Approaching the metadata limit ({{ metadataRows().length }}/50).</app-inline-alert
            >
          }
          @if (metadataRows().length >= 50) {
            <app-inline-alert tone="error">Maximum 50 metadata entries.</app-inline-alert>
          }
        </fieldset>

        @if (crossFieldError(); as err) {
          <app-inline-alert tone="error">{{ err }}</app-inline-alert>
        }
        @if (contactError(); as err) {
          <app-inline-alert tone="error">{{ err }}</app-inline-alert>
        }
        @if (conflictError(); as err) {
          <app-inline-alert tone="error">{{ err }}</app-inline-alert>
        }
        @for (err of formLevelErrors(); track $index) {
          <app-inline-alert tone="error">
            @if (err.field) {
              <strong>{{ err.field }}:</strong>
            }
            {{ err.message }}</app-inline-alert
          >
        }

        <div class="actions">
          <app-button variant="secondary" (pressed)="closeDialog.emit()">Cancel</app-button>
          <app-button variant="primary" type="submit" [disabled]="submitting()">
            {{
              submitting() ? 'Saving…' : mode() === 'create' ? 'Create customer' : 'Save changes'
            }}
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
      }
      fieldset {
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        padding: var(--app-space-3);
        margin-bottom: var(--app-space-4);
      }
      legend {
        font-weight: 700;
        font-size: var(--app-font-sm);
        color: var(--app-text);
        padding: 0 var(--app-space-1);
      }
      .identifier-row,
      .metadata-row {
        display: flex;
        gap: var(--app-space-2);
        align-items: flex-start;
        margin-bottom: var(--app-space-2);
      }
      .identifier-row app-form-field,
      .metadata-row app-form-field {
        flex: 1;
        margin-bottom: 0;
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
export class CustomerDialogComponent {
  readonly mode = input<CustomerDialogMode>('create');
  readonly customer = input<CustomerDetail | null>(null);
  readonly submitting = input(false);
  readonly error = input<ApiError | null>(null);
  readonly closeDialog = output<void>();
  readonly create = output<CreateCustomerPayload>();
  readonly update = output<UpdateCustomerPayload>();

  private readonly fb = inject(FormBuilder);

  protected readonly channels = CHANNELS;

  protected readonly form = this.fb.group(
    {
      displayName: this.fb.nonNullable.control('', [
        Validators.required,
        Validators.maxLength(200),
      ]),
      email: this.fb.control('', [Validators.email, Validators.maxLength(320)]),
      phone: this.fb.control('', [phoneValidator]),
      identifiers: this.fb.array<FormGroup>([]),
      metadata: this.fb.array<FormGroup>([]),
    },
    { validators: contactOrIdentifierValidator },
  );

  private readonly identifierVersion = signal(0);
  private readonly metadataVersion = signal(0);

  private readonly initialValues = signal<{
    displayName: string;
    email: string | null;
    phone: string | null;
    identifiers: Array<{ channel: string; identifier: string }>;
    metadata: Record<string, string>;
  } | null>(null);

  protected readonly identifierRows = computed(() => {
    this.identifierVersion();
    return this.form.controls.identifiers.controls.map((_, index) => ({ index }));
  });

  protected readonly metadataRows = computed(() => {
    this.metadataVersion();
    return this.form.controls.metadata.controls.map((_, index) => ({ index }));
  });

  constructor() {
    effect(() => {
      const customer = this.customer();
      if (!customer) {
        this.initialValues.set(null);
        return;
      }
      this.form.patchValue({
        displayName: customer.displayName,
        email: customer.email ?? '',
        phone: customer.phone ?? '',
      });
      while (this.form.controls.identifiers.length) {
        this.form.controls.identifiers.removeAt(0);
      }
      for (const id of customer.identifiers) {
        this.addIdentifier(id.channel, id.identifier);
      }
      while (this.form.controls.metadata.length) {
        this.form.controls.metadata.removeAt(0);
      }
      for (const [key, value] of Object.entries(customer.metadata ?? {})) {
        this.addMetadata(key, value);
      }
      this.initialValues.set({
        displayName: customer.displayName,
        email: customer.email ?? null,
        phone: customer.phone ?? null,
        identifiers: customer.identifiers.map((i) => ({
          channel: i.channel,
          identifier: i.identifier,
        })),
        metadata: customer.metadata ?? {},
      });
    });

    effect(() => {
      this.error();
      this.applyServerErrors();
    });

    this.form.valueChanges.pipe(takeUntilDestroyed()).subscribe(() => {
      this.clearServerErrors();
    });
  }

  protected channelLabel(channel: string): string {
    return CHANNEL_LABELS[channel] ?? channel;
  }

  protected addIdentifier(channel: ChannelIdentifier['channel'] = 'email', identifier = ''): void {
    const group = this.fb.nonNullable.group(
      {
        channel: [channel, Validators.required],
        identifier: [identifier, [Validators.required, Validators.maxLength(320)]],
      },
      { validators: channelIdentifierValidator },
    );
    this.form.controls.identifiers.push(group);
    this.identifierVersion.update((v) => v + 1);
  }

  protected removeIdentifier(index: number): void {
    this.form.controls.identifiers.removeAt(index);
    this.identifierVersion.update((v) => v + 1);
  }

  protected addMetadata(key = '', value = ''): void {
    if (this.form.controls.metadata.length >= 50) return;
    const group = this.fb.nonNullable.group({
      key: [key, [Validators.required, Validators.maxLength(100)]],
      value: [value, [Validators.required, Validators.maxLength(500)]],
    });
    this.form.controls.metadata.push(group);
    this.metadataVersion.update((v) => v + 1);
  }

  protected removeMetadata(index: number): void {
    this.form.controls.metadata.removeAt(index);
    this.metadataVersion.update((v) => v + 1);
  }

  protected fieldError(field: string): string | null {
    const err = this.error();
    if (!err?.details) return null;
    const detail = err.details.find((d) => d.field === field);
    return detail?.message ?? null;
  }

  protected identifierError(index: number, field: string): string | null {
    this.error();
    const group = this.form.controls.identifiers.at(index) as FormGroup | null;
    if (!group) return null;
    const serverErr = group.controls[field]?.errors?.['server'];
    if (serverErr) return serverErr;
    if (field === 'identifier') {
      const channelErr = group.errors?.['channelIdentifier'] as string | undefined;
      if (channelErr) return channelErr;
    }
    return null;
  }

  protected metadataError(index: number): string | null {
    this.error();
    const group = this.form.controls.metadata.at(index) as FormGroup | null;
    return group?.controls['value']?.errors?.['server'] ?? null;
  }

  private readonly unhandledErrors = signal<readonly { field: string; message: string }[]>([]);

  protected readonly errorState = computed(() => {
    const err = this.error();
    if (!err) return null;

    const details = err.details ?? [];
    const simpleFields = new Set(['displayName', 'email', 'phone']);
    const hasFieldLevelError = details.some((d) => d.field && simpleFields.has(d.field));

    return {
      code: err.code,
      message: err.message,
      status: err.status,
      conflict: err.code === 'conflict',
      conflictMessage:
        err.code === 'conflict'
          ? (details.find((d) => d.field === 'identifiers')?.message ?? err.message)
          : null,
      contactMessage:
        err.code === 'validation_failed' && !hasFieldLevelError
          ? (details.find((d) => d.field === 'contact')?.message ?? null)
          : null,
      unhandled: this.unhandledErrors(),
    };
  });

  private clearServerErrors(): void {
    const clear = (control: AbstractControl): void => {
      if (control instanceof FormGroup) {
        for (const c of Object.values(control.controls)) {
          clear(c);
        }
      } else if (control instanceof FormArray) {
        for (const c of control.controls) {
          clear(c);
        }
      } else if (control.errors?.['server']) {
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        const { server: _, ...rest } = control.errors;
        control.setErrors(Object.keys(rest).length ? rest : null);
      }
    };
    clear(this.form);
    this.unhandledErrors.set([]);
  }

  private applyServerErrors(): void {
    this.clearServerErrors();
    const err = this.error();
    if (!err?.details) return;

    const unconsumed: { field: string; message: string }[] = [];

    for (const detail of err.details) {
      if (!detail.field) {
        unconsumed.push({ field: '', message: detail.message });
        continue;
      }

      const idMatch = detail.field.match(/^identifiers\[(\d+)]\.(\w+)$/);
      if (idMatch) {
        const idx = Number(idMatch[1]);
        const field = idMatch[2];
        const group = this.form.controls.identifiers.at(idx) as FormGroup | null;
        if (group?.controls[field]) {
          group.controls[field].setErrors({ server: detail.message });
        } else {
          unconsumed.push({ field: detail.field, message: detail.message });
        }
        continue;
      }

      const metaMatch = detail.field.match(/^metadata\[(.+)]$/);
      if (metaMatch) {
        const key = metaMatch[1];
        const rows = this.form.controls.metadata.controls as FormGroup[];
        const row = rows.find((r) => r.controls['key']?.value === key);
        if (row?.controls['value']) {
          row.controls['value'].setErrors({ server: detail.message });
        } else {
          unconsumed.push({ field: detail.field, message: detail.message });
        }
        continue;
      }

      if (detail.field in this.form.controls) {
        const ctrl = (this.form.controls as Record<string, AbstractControl>)[detail.field];
        ctrl.setErrors({ server: detail.message });
      } else {
        unconsumed.push({ field: detail.field, message: detail.message });
      }
    }

    this.unhandledErrors.set(unconsumed);
  }

  protected readonly crossFieldError = computed(() => {
    if (this.form.hasError('contactOrIdentifier') && (this.form.dirty || this.form.touched)) {
      return this.form.getError('contactOrIdentifier') as string;
    }
    return null;
  });

  protected readonly metadataApproachingLimit = computed(
    () => this.metadataRows().length >= 45 && this.metadataRows().length < 50,
  );

  protected contactError = computed(() => {
    const err = this.error();
    if (!err?.details) return null;
    const contactFields = ['displayName', 'email', 'phone'];
    const hasContactDetail = err.details.some((d) => contactFields.includes(d.field ?? ''));
    if (hasContactDetail) return null;
    if (err.code === 'validation_failed' && err.details.some((d) => d.field === 'contact')) {
      const detail = err.details.find((d) => d.field === 'contact');
      return (
        detail?.message ?? 'At least one contact method (email, phone, or identifier) is required.'
      );
    }
    return null;
  });

  protected readonly formLevelErrors = computed(() => {
    const err = this.error();
    if (!err) return [];

    const details = err.details ?? [];

    if (details.length === 0) {
      if (err.status >= 400) {
        return [{ field: '', message: err.message }];
      }
      return [];
    }

    const fieldControls = new Set(['displayName', 'email', 'phone']);
    const unhandled = this.unhandledErrors();
    if (unhandled.length > 0) {
      return [...unhandled];
    }

    return details
      .filter((d) => {
        if (!d.field) return true;
        if (fieldControls.has(d.field)) return false;
        if (/^identifiers\[\d+]\.\w+$/.test(d.field)) return false;
        if (/^identifiers\[\d+]$/.test(d.field)) return false;
        if (/^metadata\[.+]$/.test(d.field)) return false;
        if (d.field === 'contact' || d.field === 'identifiers') return false;
        if (d.code === 'identifier_conflict') return false;
        return true;
      })
      .map((d) => ({ field: d.field ?? '', message: d.message }));
  });

  protected conflictError = computed(() => {
    const err = this.error();
    if (!err) return null;
    if (err.code === 'conflict') {
      const detail = err.details?.find((d) => d.field === 'identifiers');
      if (detail) {
        return detail.message;
      }
      return err.message;
    }
    return null;
  });

  protected submit(): void {
    if (this.form.invalid) {
      this.form.markAllAsTouched();
      return;
    }

    const raw = this.form.getRawValue();

    const rawIdentifiers = raw.identifiers as Array<{ channel: string; identifier: string } | null>;
    const identifiers = rawIdentifiers
      .filter(
        (id): id is { channel: string; identifier: string } =>
          id != null && !!id.channel && !!id.identifier,
      )
      .map((id) => ({
        channel: id.channel as ChannelIdentifier['channel'],
        identifier: id.identifier,
      }));

    const rawMetadata = raw.metadata as Array<{ key: string; value: string } | null>;
    const metadata: Record<string, string> = {};
    for (const entry of rawMetadata) {
      if (entry && entry.key && entry.value) {
        metadata[entry.key] = entry.value;
      }
    }

    if (this.mode() === 'edit') {
      const initial = this.initialValues();
      if (initial) {
        const emailValue = raw.email || null;
        const phoneValue = raw.phone || null;
        const payload: UpdateCustomerPayload = {
          ...(raw.displayName !== initial.displayName ? { displayName: raw.displayName } : {}),
          ...(emailValue !== initial.email ? { email: emailValue } : {}),
          ...(phoneValue !== initial.phone ? { phone: phoneValue } : {}),
        };
        const currentIdentifiers = identifiers.length > 0 ? identifiers : undefined;
        const initialIdentifiers = initial.identifiers.length > 0 ? initial.identifiers : undefined;
        if (JSON.stringify(currentIdentifiers ?? []) !== JSON.stringify(initialIdentifiers ?? [])) {
          (payload as Record<string, unknown>)['identifiers'] = identifiers;
        }
        if (JSON.stringify(metadata) !== JSON.stringify(initial.metadata)) {
          (payload as Record<string, unknown>)['metadata'] = metadata;
        }
        this.update.emit(payload);
      } else {
        this.update.emit({
          displayName: raw.displayName,
          email: raw.email || null,
          phone: raw.phone || null,
          identifiers,
          metadata,
        });
      }
    } else {
      const payload: CreateCustomerPayload = {
        displayName: raw.displayName,
        ...(raw.email ? { email: raw.email } : {}),
        ...(raw.phone ? { phone: raw.phone } : {}),
        ...(identifiers.length > 0 ? { identifiers } : {}),
        ...(Object.keys(metadata).length > 0 ? { metadata } : {}),
      };
      this.create.emit(payload);
    }
  }
}
