import { ApiError } from '../../../core/api/api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { marker } from '../../../core/i18n/marker';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { FormBuilder, ReactiveFormsModule, Validators } from '@angular/forms';
import {
  CreateInvitationPayload,
  CreateInvitationResponse,
  MembershipRole,
} from '../../../core/api/tenant-api.models';
import { APP_CONFIG } from '../../../core/config/app-config';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DialogShellComponent } from '../../../shared/components/dialog-shell/dialog-shell.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { IconButtonComponent } from '../../../shared/components/icon-button/icon-button.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { copyToClipboard } from '../../../shared/utils/clipboard';
import { RoleSelectComponent } from './role-select.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

@Component({
  selector: 'app-invite-dialog',
  providers: [provideTranslocoScope('team')],
  imports: [
    ButtonComponent,
    DialogShellComponent,
    FormFieldComponent,
    IconButtonComponent,
    InlineAlertComponent,
    ReactiveFormsModule,
    RoleSelectComponent,
    TranslocoPipe,
  ],
  templateUrl: './invite-dialog.component.html',
  styles: [
    `
      h3 {
        margin: 0 0 1rem;
        font-size: 1.125rem;
      }
      app-form-field {
        margin-bottom: 1rem;
      }
      .actions {
        display: flex;
        gap: 0.5rem;
        justify-content: flex-end;
        margin-top: 1rem;
      }
      .sr-only {
        position: absolute;
        width: 1px;
        height: 1px;
        padding: 0;
        margin: -1px;
        overflow: hidden;
        clip: rect(0, 0, 0, 0);
        white-space: nowrap;
        border: 0;
      }
      .link-box {
        background: var(--tui-background-neutral-1);
        padding: 0.75rem;
        border-radius: 0.5rem;
        display: flex;
        gap: 0.75rem;
        align-items: center;
        justify-content: space-between;
        font-size: 0.875rem;
        margin: 1rem 0;
      }
      .link-box input {
        min-width: 0;
        flex: 1;
        border: 0;
        background: transparent;
        color: var(--app-text);
        font-family: monospace;
        font-size: inherit;
      }
      .link-instructions {
        margin: 1rem 0 0;
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class InviteDialogComponent {
  readonly invite = output<CreateInvitationPayload>();
  readonly closeDialog = output<void>();

  readonly submitting = input(false);
  /** What went wrong sending the invitation, kept whole for its text and request reference. */
  readonly error = input<ApiError | null>(null);
  readonly result = input<CreateInvitationResponse | null>(null);
  /** The delivery status of the invitation email could not be refreshed. */
  readonly deliveryPollingFailed = input(false);
  readonly currentRole = input<MembershipRole>('viewer');
  readonly canAssignOwner = input(false);

  private readonly fb = inject(FormBuilder);
  private readonly appConfig = inject(APP_CONFIG);
  private copyRequest = 0;
  protected readonly form = this.fb.nonNullable.group({
    email: ['', [Validators.required, Validators.email, Validators.maxLength(254)]],
    role: ['agent' as MembershipRole, Validators.required],
  });

  protected readonly step = computed(() => (this.result() ? 'result' : 'form'));
  protected readonly copyStatus = signal<'copied' | 'failed' | null>(null);
  protected readonly acceptUrl = computed(() => {
    const url = this.result()?.acceptUrl;
    if (!url) return '';

    try {
      return new URL(url, this.appConfig.publicDashboardUrl).toString();
    } catch {
      return url;
    }
  });
  protected submit(): void {
    if (this.form.invalid) {
      this.form.markAllAsTouched();
      return;
    }

    const { email, role } = this.form.getRawValue();
    this.invite.emit({ email, role });
  }

  protected async copyLink(): Promise<void> {
    const acceptUrl = this.acceptUrl();
    if (!acceptUrl) return;

    const request = ++this.copyRequest;
    this.copyStatus.set(null);
    try {
      await copyToClipboard(acceptUrl);
      if (request === this.copyRequest) this.copyStatus.set('copied');
    } catch {
      if (request === this.copyRequest) this.copyStatus.set('failed');
    }
  }
  /** Catalog key of what happened to the invitation email; it takes the invited address. */
  protected deliveryMessage(): string {
    switch (this.result()?.emailDeliveryStatus) {
      case 'sent':
        return marker('team.invite.delivery.sent');
      case 'queued':
        return marker('team.invite.delivery.queued');
      case 'failed':
        return marker('team.invite.delivery.failed');
      case 'unconfigured':
      default:
        return marker('team.invite.delivery.unconfigured');
    }
  }

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }
}
