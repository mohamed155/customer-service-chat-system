import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  output,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { TuiIcon } from '@taiga-ui/core';
import { ApiError } from '../../../core/api/api.models';
import { ErrorKeyOverrides, errorMessageKey } from '../../../core/errors/error-message';
import { marker } from '../../../core/i18n/marker';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { ButtonComponent } from '../../../shared/components/button/button.component';
import { DialogShellComponent } from '../../../shared/components/dialog-shell/dialog-shell.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { IconButtonComponent } from '../../../shared/components/icon-button/icon-button.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { KnowledgeStore } from './knowledge.store';

/** A category's name is unique in a workspace; the API answers a repeated one with a conflict. */
const CATEGORY_ERRORS: ErrorKeyOverrides = {
  conflict: marker('knowledge-base.categories.duplicate'),
};

@Component({
  selector: 'app-category-manager',
  imports: [
    ButtonComponent,
    DialogShellComponent,
    FormFieldComponent,
    FormsModule,
    IconButtonComponent,
    InlineAlertComponent,
    TranslocoPipe,
    TuiIcon,
  ],
  providers: [provideTranslocoScope('knowledge-base')],
  templateUrl: './category-manager.component.html',
  styles: [
    `
      h3 {
        margin: 0 0 var(--app-space-4);
        font-size: 1.125rem;
      }
      .cat-list {
        display: grid;
        gap: var(--app-space-2);
        margin-bottom: var(--app-space-4);
      }
      .cat-row {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: var(--app-space-2);
        padding: var(--app-space-2) var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
      }
      .cat-name {
        font-weight: 600;
        font-size: var(--app-font-sm);
        color: var(--app-text);
      }
      .cat-actions {
        display: flex;
        gap: var(--app-space-1);
      }
      .inline-edit {
        display: flex;
        gap: var(--app-space-2);
        align-items: center;
        flex: 1;
      }
      .inline-edit input {
        flex: 1;
        min-width: 0;
        padding: var(--app-space-1) var(--app-space-2);
        border: 1px solid var(--app-border-strong);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font: inherit;
        font-size: var(--app-font-sm);
      }
      .delete-confirm {
        display: grid;
        gap: var(--app-space-2);
        flex: 1;
      }
      .delete-confirm span {
        font-weight: 600;
        font-size: var(--app-font-sm);
      }
      .uncategorized-note {
        margin: 0;
        font-size: var(--app-font-xs);
        color: var(--app-text-2);
        font-style: italic;
      }
      .confirm-actions {
        display: flex;
        gap: var(--app-space-2);
      }
      .add-form {
        display: grid;
        gap: var(--app-space-3);
        margin-bottom: var(--app-space-4);
      }
      .add-actions {
        display: flex;
        gap: var(--app-space-2);
      }
      .add-btn-row {
        margin-bottom: var(--app-space-4);
      }
      .footer-actions {
        display: flex;
        justify-content: flex-end;
      }
      .empty-text {
        color: var(--app-text-3);
        font-size: var(--app-font-sm);
        text-align: center;
        padding: var(--app-space-4);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CategoryManagerComponent {
  readonly store = inject(KnowledgeStore);
  private readonly permissions = inject(PermissionsService);

  readonly closed = output<void>();

  protected readonly canManage = computed(() => this.permissions.has('knowledge_base.manage'));
  protected readonly showAddForm = signal(false);
  protected readonly addName = signal('');
  protected readonly editingId = signal<string | null>(null);
  protected readonly editName = signal('');
  protected readonly deletingId = signal<string | null>(null);

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error, CATEGORY_ERRORS);
  }

  protected startEdit(cat: { id: string; name: string }): void {
    this.editingId.set(cat.id);
    this.editName.set(cat.name);
    this.deletingId.set(null);
  }

  protected cancelEdit(): void {
    this.editingId.set(null);
    this.editName.set('');
  }

  protected saveRename(id: string): void {
    const name = this.editName().trim();
    if (!name) return;
    this.store.renameCategory(id, name);
    this.cancelEdit();
  }

  protected startDelete(id: string): void {
    this.deletingId.set(id);
    this.editingId.set(null);
    this.showAddForm.set(false);
  }

  protected cancelDelete(): void {
    this.deletingId.set(null);
  }

  protected confirmDelete(id: string): void {
    this.store.deleteCategory(id);
    this.cancelDelete();
  }

  protected saveAdd(): void {
    const name = this.addName().trim();
    if (!name) return;
    this.store.createCategory(name);
    this.addName.set('');
    this.showAddForm.set(false);
  }

  protected cancelAdd(): void {
    this.showAddForm.set(false);
    this.addName.set('');
  }
}
