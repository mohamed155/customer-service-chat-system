import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { finalize } from 'rxjs/operators';
import { ApiError } from '../../../core/api/api.models';
import { toApiError } from '../../../core/errors/api-error';
import { ErrorKeyOverrides, errorMessageKey } from '../../../core/errors/error-message';
import { marker } from '../../../core/i18n/marker';
import { KnowledgeItemDetail } from '../../../core/api/knowledge.models';
import { DialogShellComponent } from '../../../shared/components/dialog-shell/dialog-shell.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { KnowledgeApiService } from './knowledge-api.service';

const ALLOWED_EXTENSIONS = ['pdf', 'docx', 'txt', 'md'];
const MAX_SIZE_BYTES = 20 * 1024 * 1024;

/** What the upload's own failures say; any other failure is worded by its code. */
const UPLOAD_ERRORS: ErrorKeyOverrides = {
  413: marker('knowledge-base.upload.serverTooLarge'),
  415: marker('knowledge-base.upload.serverUnsupported'),
};

/** What a form message says: a catalog key and the values its text needs. */
interface Message {
  readonly key: string;
  readonly params?: Record<string, unknown>;
}

@Component({
  selector: 'app-upload-document',
  imports: [
    DialogShellComponent,
    FormsModule,
    FormFieldComponent,
    InlineAlertComponent,
    TranslocoPipe,
  ],
  providers: [provideTranslocoScope('knowledge-base')],
  templateUrl: './upload-document.component.html',
  styles: [
    `
      .upload-dialog {
        display: grid;
        gap: var(--app-space-4);
      }
      .dialog-title {
        margin: 0;
        font-size: var(--app-font-lg);
        font-weight: 700;
        color: var(--app-text);
      }
      .file-info {
        margin-top: calc(-1 * var(--app-space-3));
        font-size: var(--app-font-xs);
        color: var(--app-text-2);
      }
      .radio-group {
        display: grid;
        gap: var(--app-space-2);
      }
      .radio {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        font-size: var(--app-font-sm);
        color: var(--app-text);
        cursor: pointer;
      }
      .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--app-space-3);
        padding-top: var(--app-space-3);
        border-top: 1px solid var(--app-border);
      }
      .btn-cancel {
        height: 38px;
        padding: 0 var(--app-space-4);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font-weight: 650;
        font-size: var(--app-font-sm);
        cursor: pointer;
      }
      .btn-cancel:hover:not(:disabled) {
        background: var(--app-panel-2);
      }
      .btn-upload {
        height: 38px;
        padding: 0 var(--app-space-5);
        border: 0;
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 650;
        font-size: var(--app-font-sm);
        cursor: pointer;
      }
      .btn-upload:disabled {
        opacity: 0.6;
        cursor: default;
      }
      .btn-upload:hover:not(:disabled) {
        opacity: 0.92;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UploadDocumentComponent {
  readonly open = input(false);
  readonly completed = output<KnowledgeItemDetail>();
  readonly closed = output<void>();

  private readonly api = inject(KnowledgeApiService);

  protected readonly title = signal('');
  protected readonly titleSource = signal<'auto' | 'manual'>('auto');
  protected readonly publishImmediately = signal(false);
  protected readonly uploading = signal(false);
  protected readonly error = signal<ApiError | null>(null);
  protected readonly validationError = signal<Message | null>(null);
  protected readonly selectedFile = signal<File | null>(null);

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error, UPLOAD_ERRORS);
  }

  /** A file's size in whole kilobytes. */
  protected kilobytes(bytes: number): number {
    return Math.round(bytes / 1024);
  }

  protected onFileSelected(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    this.validationError.set(null);
    this.error.set(null);

    const ext = file.name.split('.').pop()?.toLowerCase() ?? '';
    if (!ALLOWED_EXTENSIONS.includes(ext)) {
      this.validationError.set({
        key: marker('knowledge-base.upload.invalidType'),
        params: { ext, allowed: ALLOWED_EXTENSIONS.join(', ') },
      });
      this.selectedFile.set(null);
      return;
    }

    if (file.size > MAX_SIZE_BYTES) {
      this.validationError.set({
        key: marker('knowledge-base.upload.tooLarge'),
        params: {
          max: MAX_SIZE_BYTES / 1024 / 1024,
          size: Math.round((file.size / 1024 / 1024) * 10) / 10,
        },
      });
      this.selectedFile.set(null);
      return;
    }

    this.selectedFile.set(file);
    if (this.titleSource() === 'auto') {
      const stem = file.name.replace(/\.[^.]+$/, '');
      this.title.set(stem);
    }
  }

  protected upload(): void {
    const file = this.selectedFile();
    if (!file || !this.title().trim()) return;

    this.uploading.set(true);
    this.error.set(null);

    const formData = new FormData();
    formData.append('file', file);
    formData.append('title', this.title());
    formData.append('publishImmediately', String(this.publishImmediately()));

    this.api
      .uploadDocument(formData)
      .pipe(finalize(() => this.uploading.set(false)))
      .subscribe({
        next: (res) => this.completed.emit(res.data),
        error: (err: unknown) => this.error.set(toApiError(err)),
      });
  }

  protected onDismiss(): void {
    if (this.uploading()) return;
    this.closed.emit();
  }
}
