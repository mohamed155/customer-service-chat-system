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
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { errorMessageKey, fieldErrorKey } from '../../../core/errors/error-message';
import { marker } from '../../../core/i18n/marker';
import { ownValue } from '../../../core/i18n/own-value';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

export type CustomerDialogMode = 'create' | 'edit';

const CHANNELS = ['email', 'phone', 'web_chat', 'whatsapp', 'telegram'] as const;

/** What a form error says: a catalog key and the values its message needs. */
export interface ErrorText {
  readonly key: string;
  readonly params?: Readonly<Record<string, unknown>>;
}

const text = (key: string, params?: Record<string, unknown>): ErrorText => ({ key, params });

function phoneValidator(control: AbstractControl): ValidationErrors | null {
  const value = control.value;
  if (!value) return null;
  const str = String(value);
  const digits = str.replace(/\D/g, '');
  // The page shows its own text for this field; the value only marks the control invalid.
  if (digits.length < 7 || digits.length > 15) return { phone: true };
  return null;
}

function channelIdentifierValidator(control: AbstractControl): ValidationErrors | null {
  const group = control as FormGroup;
  const channel = group.get('channel')?.value as string | null;
  const identifier = group.get('identifier')?.value as string | null;
  if (!channel || !identifier) return null;

  if (!identifier.trim()) {
    return { channelIdentifier: text(marker('customers.dialog.errors.identifierRequired')) };
  }

  if (channel === 'email') {
    const parts = identifier.split('@');
    if (parts.length !== 2 || !parts[0] || !parts[1]) {
      return { channelIdentifier: text(marker('customers.dialog.errors.identifierEmail')) };
    }
    const local = parts[0];
    const domain = parts[1];
    if (local.length > 64) {
      return { channelIdentifier: text(marker('customers.dialog.errors.identifierEmailLocal')) };
    }
    const labels = domain.split('.');
    if (labels.length < 2 || labels.some((l) => !l || l.length > 63)) {
      return { channelIdentifier: text(marker('customers.dialog.errors.identifierEmail')) };
    }
    const tld = labels[labels.length - 1];
    if (tld.length < 2) {
      return { channelIdentifier: text(marker('customers.dialog.errors.identifierEmail')) };
    }
  }

  if (channel === 'phone' || channel === 'whatsapp') {
    const str = String(identifier);
    const digits = str.replace(/\D/g, '');
    if (digits.length < 7 || digits.length > 15) {
      return {
        channelIdentifier: text(marker('customers.dialog.errors.identifierPhone'), { channel }),
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
  return { contactOrIdentifier: text(marker('customers.dialog.errors.contactRequired')) };
}

/** Catalog keys (shared) of the channel names. */
const CHANNEL_LABEL_KEYS: Readonly<Record<string, string>> = {
  email: marker('channels.email'),
  phone: marker('channels.phone'),
  web_chat: marker('channels.web_chat'),
  whatsapp: marker('channels.whatsapp'),
  telegram: marker('channels.telegram'),
};

@Component({
  selector: 'app-customer-dialog',
  providers: [provideTranslocoScope('customers')],
  imports: [
    ErrorReferenceComponent,
    ButtonComponent,
    DialogShellComponent,
    FormFieldComponent,
    IconButtonComponent,
    InlineAlertComponent,
    ReactiveFormsModule,
    TranslocoPipe,
  ],
  templateUrl: './customer-dialog.component.html',
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
    return ownValue(CHANNEL_LABEL_KEYS, channel) ?? channel;
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

  /** What the API said about a field: the text for its error code, never its own message. */
  protected fieldError(field: string): ErrorText | null {
    const err = this.error();
    if (!err?.details) return null;
    const detail = err.details.find((d) => d.field === field);
    return detail ? text(fieldErrorKey(detail.code)) : null;
  }

  protected identifierError(index: number, field: string): ErrorText | null {
    this.error();
    const group = this.form.controls.identifiers.at(index) as FormGroup | null;
    if (!group) return null;
    const serverErr = group.controls[field]?.errors?.['server'] as ErrorText | undefined;
    if (serverErr) return serverErr;
    if (field === 'identifier') {
      const channelErr = group.errors?.['channelIdentifier'] as ErrorText | undefined;
      if (channelErr) return channelErr;
    }
    return null;
  }

  protected metadataError(index: number): ErrorText | null {
    this.error();
    const group = this.form.controls.metadata.at(index) as FormGroup | null;
    return (group?.controls['value']?.errors?.['server'] as ErrorText | undefined) ?? null;
  }

  private readonly unhandledErrors = signal<readonly { field: string; key: string }[]>([]);

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

    const unconsumed: { field: string; key: string }[] = [];

    for (const detail of err.details) {
      if (!detail.field) {
        unconsumed.push({ field: '', key: fieldErrorKey(detail.code) });
        continue;
      }

      const idMatch = detail.field.match(/^identifiers\[(\d+)]\.(\w+)$/);
      if (idMatch) {
        const idx = Number(idMatch[1]);
        const field = idMatch[2];
        const group = this.form.controls.identifiers.at(idx) as FormGroup | null;
        if (group?.controls[field]) {
          group.controls[field].setErrors({ server: text(fieldErrorKey(detail.code)) });
        } else {
          unconsumed.push({ field: detail.field, key: fieldErrorKey(detail.code) });
        }
        continue;
      }

      const metaMatch = detail.field.match(/^metadata\[(.+)]$/);
      if (metaMatch) {
        const key = metaMatch[1];
        const rows = this.form.controls.metadata.controls as FormGroup[];
        const row = rows.find((r) => r.controls['key']?.value === key);
        if (row?.controls['value']) {
          row.controls['value'].setErrors({ server: text(fieldErrorKey(detail.code)) });
        } else {
          unconsumed.push({ field: detail.field, key: fieldErrorKey(detail.code) });
        }
        continue;
      }

      if (detail.field in this.form.controls) {
        const ctrl = (this.form.controls as Record<string, AbstractControl>)[detail.field];
        ctrl.setErrors({ server: text(fieldErrorKey(detail.code)) });
      } else {
        unconsumed.push({ field: detail.field, key: fieldErrorKey(detail.code) });
      }
    }

    this.unhandledErrors.set(unconsumed);
  }

  protected readonly crossFieldError = computed<ErrorText | null>(() => {
    if (this.form.hasError('contactOrIdentifier') && (this.form.dirty || this.form.touched)) {
      return this.form.getError('contactOrIdentifier') as ErrorText;
    }
    return null;
  });

  protected readonly metadataApproachingLimit = computed(
    () => this.metadataRows().length >= 45 && this.metadataRows().length < 50,
  );

  protected contactError = computed<ErrorText | null>(() => {
    const err = this.error();
    if (!err?.details) return null;
    const contactFields = ['displayName', 'email', 'phone'];
    const hasContactDetail = err.details.some((d) => contactFields.includes(d.field ?? ''));
    if (hasContactDetail) return null;
    if (err.code === 'validation_failed' && err.details.some((d) => d.field === 'contact')) {
      return text(marker('customers.dialog.errors.contactRequired'));
    }
    return null;
  });

  protected readonly formLevelErrors = computed(() => {
    const err = this.error();
    if (!err) return [];

    const details = err.details ?? [];

    if (details.length === 0) {
      // conflictError() already renders a bare conflict; don't show it twice.
      if (err.status >= 400 && err.code !== 'conflict') {
        return [{ field: '', key: errorMessageKey(err) }];
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
      .map((d) => ({ field: d.field ?? '', key: fieldErrorKey(d.code) }));
  });

  protected conflictError = computed<ErrorText | null>(() => {
    const err = this.error();
    if (!err) return null;
    if (err.code === 'conflict') {
      const detail = err.details?.find((d) => d.field === 'identifiers');
      if (!detail) return text(errorMessageKey(err));
      // The API names the customer who holds the identifier in separate fields; those are data.
      const held = detail as unknown as Record<string, unknown>;
      const { channel, identifier, existing_customer_name: name } = held;
      return typeof channel === 'string' &&
        typeof identifier === 'string' &&
        typeof name === 'string'
        ? text(marker('customers.dialog.errors.identifierHeld'), { channel, identifier, name })
        : text(marker('customers.dialog.errors.identifierConflict'));
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
