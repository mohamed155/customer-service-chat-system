import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { TranslocoPipe } from '@jsverse/transloco';
import { ApiError } from '../../core/api/api.models';
import { toApiError } from '../../core/errors/api-error';
import { errorMessageKey, fieldErrorKey } from '../../core/errors/error-message';
import { marker } from '../../core/i18n/marker';
import { TenantContextService } from '../../core/tenant/tenant-context.service';
import { ButtonComponent } from '../../shared/components/button/button.component';
import { DialogShellComponent } from '../../shared/components/dialog-shell/dialog-shell.component';
import { InlineAlertComponent } from '../../shared/components/inline-alert/inline-alert.component';
import { ErrorReferenceComponent } from '../../shared/components/error-reference/error-reference.component';

let nextId = 0;

@Component({
  selector: 'app-platform-access-dialog',
  imports: [
    ButtonComponent,
    DialogShellComponent,
    ErrorReferenceComponent,
    InlineAlertComponent,
    TranslocoPipe,
  ],
  templateUrl: './platform-access-dialog.component.html',
  styles: [
    `
      h2 {
        margin: 0 0 var(--app-space-3);
        font-size: var(--app-font-lg);
      }
      form {
        display: grid;
        gap: var(--app-space-3);
      }
      label {
        display: block;
        margin-block-end: var(--app-space-1);
        font-weight: 600;
      }
      textarea,
      select {
        box-sizing: border-box;
        width: 100%;
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font: inherit;
      }
      textarea {
        resize: vertical;
      }
      textarea:focus-visible,
      select:focus-visible {
        outline: 2px solid var(--app-accent);
        outline-offset: 2px;
      }
      .hint,
      .field-error {
        display: block;
        margin-block-start: var(--app-space-1);
        font-size: var(--app-font-sm);
      }
      .hint {
        color: var(--app-text-2);
      }
      .field-error {
        color: var(--app-red-text);
      }
      .actions {
        display: flex;
        flex-wrap: wrap;
        justify-content: flex-end;
        gap: var(--app-space-2);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PlatformAccessDialogComponent {
  private readonly tenantContext = inject(TenantContextService);

  readonly tenant = input.required<{ id: string; name: string }>();
  readonly closed = output<void>();
  readonly started = output<void>();

  protected readonly id = `platform-access-${nextId++}`;
  protected readonly reason = signal('');
  protected readonly durationMinutes = signal(60);
  protected readonly durations = [5, 15, 30, 60, 120, 180, 240];
  protected readonly error = signal<ApiError | null>(null);
  protected readonly pending = signal(false);
  protected readonly submitted = signal(false);
  protected readonly reasonLength = computed(() => Array.from(this.reason().trim()).length);
  protected readonly reasonError = computed(() => {
    const code = this.error()?.details?.find((detail) => detail.field === 'reason')?.code;
    const reasonLength = this.reasonLength();
    if (code === 'too_short') return marker('platformAccess.reasonTooShort');
    if (code === 'too_long') return marker('platformAccess.reasonTooLong');
    if (code) return fieldErrorKey(code);
    if (!this.submitted()) return null;
    if (reasonLength === 0) return fieldErrorKey('required');
    if (reasonLength < 10) return marker('platformAccess.reasonTooShort');
    if (reasonLength > 500) return marker('platformAccess.reasonTooLong');
    return null;
  });
  protected readonly durationError = computed(() =>
    this.error()?.details?.some((detail) => detail.field === 'durationMinutes')
      ? marker('platformAccess.durationInvalid')
      : null,
  );
  protected readonly requestError = computed(() => {
    const error = this.error();
    return error && !this.reasonError() && !this.durationError() ? errorMessageKey(error) : null;
  });

  protected changeReason(value: string): void {
    this.reason.set(value);
    this.clearFieldError('reason');
  }

  protected changeDuration(value: string): void {
    this.durationMinutes.set(Number(value));
    this.clearFieldError('durationMinutes');
  }

  protected async submit(): Promise<void> {
    if (this.pending()) return;
    this.submitted.set(true);
    this.error.set(null);
    if (this.reasonError()) return;

    this.pending.set(true);
    try {
      await this.tenantContext.select(this.tenant().id, {
        reason: this.reason().trim(),
        durationMinutes: this.durationMinutes(),
      });
      this.started.emit();
      this.closed.emit();
    } catch (error: unknown) {
      this.error.set(toApiError(error));
    } finally {
      this.pending.set(false);
    }
  }

  private clearFieldError(field: string): void {
    this.error.update((error) => {
      if (!error?.details?.some((detail) => detail.field === field)) return error;
      const details = error.details.filter((detail) => detail.field !== field);
      return details.length ? { ...error, details } : null;
    });
  }
}
