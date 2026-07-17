import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { finalize } from 'rxjs/operators';
import { KnowledgeItemDetail } from '../../../core/api/knowledge.models';
import { DialogShellComponent } from '../../../shared/components/dialog-shell/dialog-shell.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { KnowledgeApiService } from './knowledge-api.service';

const ALLOWED_EXTENSIONS = ['pdf', 'docx', 'txt', 'md'];
const MAX_SIZE_BYTES = 20 * 1024 * 1024;

@Component({
  selector: 'app-upload-document',
  imports: [DialogShellComponent, FormsModule, FormFieldComponent, InlineAlertComponent],
  template: `
    <app-dialog-shell [open]="open()" (dismiss)="onDismiss()" ariaLabelledby="upload-title">
      <div class="upload-dialog">
        <h2 id="upload-title" class="dialog-title">Upload document</h2>

        @if (error(); as err) {
          <app-inline-alert tone="error">{{ err }}</app-inline-alert>
        }

        @if (validationError(); as verr) {
          <app-inline-alert tone="error">{{ verr }}</app-inline-alert>
        }

        <app-form-field label="File" for="file">
          <input
            id="file"
            type="file"
            accept=".pdf,.docx,.txt,.md"
            (change)="onFileSelected($event)"
            [disabled]="uploading()"
          />
        </app-form-field>

        @if (selectedFile(); as file) {
          <div class="file-info">{{ file.name }} ({{ (file.size / 1024).toFixed(0) }} KB)</div>
        }

        <app-form-field label="Title" for="title">
          <input
            id="title"
            type="text"
            [ngModel]="title()"
            (ngModelChange)="title.set($event); titleSource.set('manual')"
            placeholder="Document title"
            [disabled]="uploading()"
          />
        </app-form-field>

        <app-form-field label="Publish">
          <div class="radio-group">
            <label class="radio">
              <input
                type="radio"
                name="publish"
                [value]="false"
                [(ngModel)]="publishImmediately"
                [disabled]="uploading()"
              />
              Save as draft
            </label>
            <label class="radio">
              <input
                type="radio"
                name="publish"
                [value]="true"
                [(ngModel)]="publishImmediately"
                [disabled]="uploading()"
              />
              Publish immediately
            </label>
          </div>
        </app-form-field>

        <div class="actions">
          <button type="button" class="btn-cancel" [disabled]="uploading()" (click)="onDismiss()">
            Cancel
          </button>
          <button
            type="button"
            class="btn-upload"
            [disabled]="uploading() || !selectedFile() || !title().trim()"
            (click)="upload()"
          >
            @if (uploading()) {
              Uploading…
            } @else {
              Upload
            }
          </button>
        </div>
      </div>
    </app-dialog-shell>
  `,
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
  protected readonly error = signal<string | null>(null);
  protected readonly validationError = signal<string | null>(null);
  protected readonly selectedFile = signal<File | null>(null);

  protected onFileSelected(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    this.validationError.set(null);
    this.error.set(null);

    const ext = file.name.split('.').pop()?.toLowerCase() ?? '';
    if (!ALLOWED_EXTENSIONS.includes(ext)) {
      this.validationError.set(
        `Invalid file type ".${ext}". Allowed: ${ALLOWED_EXTENSIONS.join(', ')}`,
      );
      this.selectedFile.set(null);
      return;
    }

    if (file.size > MAX_SIZE_BYTES) {
      this.validationError.set(
        `File exceeds 20 MB limit (${(file.size / 1024 / 1024).toFixed(1)} MB)`,
      );
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
        error: (err: unknown) =>
          this.error.set((err as { message?: string })?.message ?? 'Upload failed'),
      });
  }

  protected onDismiss(): void {
    if (this.uploading()) return;
    this.closed.emit();
  }
}
