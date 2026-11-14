import { ChangeDetectionStrategy, Component, effect, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router } from '@angular/router';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { TuiIcon } from '@taiga-ui/core';
import { ApiError } from '../../../core/api/api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { KnowledgeItemType } from '../../../core/api/knowledge.models';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { FormFieldComponent } from '../../../shared/components/form-field/form-field.component';
import { InlineAlertComponent } from '../../../shared/components/inline-alert/inline-alert.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { RichTextEditorComponent } from './rich-text-editor.component';
import { KnowledgeStore } from './knowledge.store';

@Component({
  selector: 'app-article-editor',
  imports: [
    DashboardCardComponent,
    FormFieldComponent,
    FormsModule,
    InlineAlertComponent,
    PageContainerComponent,
    PageHeaderComponent,
    RichTextEditorComponent,
    TranslocoPipe,
    TuiIcon,
  ],
  providers: [KnowledgeStore, provideTranslocoScope('knowledge-base')],
  templateUrl: './article-editor.component.html',
  styles: [
    `
      .loading {
        padding: var(--app-space-8);
        text-align: center;
        color: var(--app-text-2);
      }
      .editor-grid {
        display: grid;
        gap: var(--app-space-4);
      }
      .field-group {
        display: grid;
        gap: var(--app-space-4);
      }
      .editor-section {
        display: grid;
        gap: var(--app-space-3);
      }
      .editor-placeholder {
        min-height: 200px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
      }
      .editor-error {
        margin: 0;
        color: var(--app-danger);
        font-size: var(--app-font-sm);
      }
      .editor-label {
        color: var(--app-text);
        font-size: var(--app-font-sm);
        font-weight: 700;
      }
      .actions-bar {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: var(--app-space-3);
        margin-top: var(--app-space-5);
        padding-top: var(--app-space-4);
        border-top: 1px solid var(--app-border);
      }
      .save-btn {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-2);
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
      .save-btn:disabled {
        opacity: 0.6;
        cursor: default;
      }
      .save-btn:hover:not(:disabled) {
        opacity: 0.92;
      }
      .saving-indicator {
        font-size: var(--app-font-sm);
        color: var(--app-text-2);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ArticleEditorComponent {
  readonly store = inject(KnowledgeStore);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);

  protected readonly isEditing = signal(false);
  protected readonly itemId = signal<string | null>(null);
  protected readonly title = signal('');
  protected readonly titleTouched = signal(false);
  protected readonly itemType = signal<KnowledgeItemType>('article');
  protected readonly categoryId = signal<string | null>(null);
  protected readonly tagsInput = signal('');
  protected readonly body = signal('');

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  constructor() {
    const id = this.route.snapshot.paramMap.get('id');
    if (id) {
      this.isEditing.set(true);
      this.itemId.set(id);
      this.store.loadItem(id);
    }

    effect(() => {
      const item = this.store.selectedItem();
      if (item && this.isEditing()) {
        this.title.set(item.title);
        this.itemType.set(item.itemType);
        this.categoryId.set(item.categoryId);
        this.tagsInput.set(item.tags.join(', '));
        this.body.set(item.body ?? '');
      }
    });
  }

  protected save(): void {
    this.titleTouched.set(true);
    if (!this.title().trim()) return;

    const tags = this.tagsInput()
      .split(',')
      .map((t) => t.trim())
      .filter(Boolean);

    const id = this.itemId();
    if (id) {
      this.store.updateItem(id, {
        title: this.title(),
        itemType: this.itemType(),
        categoryId: this.categoryId(),
        tags,
        body: this.body(),
      });
    } else {
      this.store.createItem({
        title: this.title(),
        itemType: this.itemType(),
        categoryId: this.categoryId(),
        tags,
        body: this.body(),
      });
    }
  }
}
