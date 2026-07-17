import { ChangeDetectionStrategy, Component, effect, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router } from '@angular/router';
import { TuiIcon } from '@taiga-ui/core';
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
    TuiIcon,
  ],
  providers: [KnowledgeStore],
  template: `
    <app-page-container>
      <app-page-header
        [title]="isEditing() ? 'Edit article' : 'New article'"
        [description]="
          isEditing() ? 'Update the knowledge article' : 'Create a new knowledge article'
        "
      />

      @if (store.loading() && isEditing()) {
        <div class="loading">Loading article…</div>
      } @else {
        <div class="editor-grid">
          <app-dashboard-card>
            <div class="field-group">
              <app-form-field label="Title" for="title">
                <input
                  id="title"
                  type="text"
                  [ngModel]="title()"
                  (ngModelChange)="title.set($event); titleTouched.set(true)"
                  placeholder="Article title"
                />
              </app-form-field>
              @if (titleTouched() && !title().trim()) {
                <app-inline-alert tone="error">Title is required</app-inline-alert>
              }

              <app-form-field label="Type" for="itemType">
                <select id="itemType" [ngModel]="itemType()" (ngModelChange)="itemType.set($event)">
                  <option value="article">Article</option>
                  <option value="faq">FAQ</option>
                </select>
              </app-form-field>

              <app-form-field label="Category" for="category">
                <select
                  id="category"
                  [ngModel]="categoryId()"
                  (ngModelChange)="categoryId.set($event)"
                >
                  <option [ngValue]="null">No category</option>
                  @for (cat of store.categories(); track cat.id) {
                    <option [value]="cat.id">{{ cat.name }} ({{ cat.itemCount }})</option>
                  }
                </select>
              </app-form-field>

              <app-form-field label="Tags" for="tags">
                <input
                  id="tags"
                  type="text"
                  [ngModel]="tagsInput()"
                  (ngModelChange)="tagsInput.set($event)"
                  placeholder="Comma-separated tags"
                />
              </app-form-field>
            </div>
          </app-dashboard-card>

          <app-dashboard-card>
            <div class="editor-section">
              <div class="editor-label">Body</div>
              <app-rich-text-editor [(value)]="body" />
            </div>
          </app-dashboard-card>
        </div>

        <div class="actions-bar">
          @if (store.saving()) {
            <span class="saving-indicator">Saving…</span>
          }
          @if (store.error(); as err) {
            <app-inline-alert tone="error">{{ err }}</app-inline-alert>
          }
          <button type="button" class="save-btn" [disabled]="store.saving()" (click)="save()">
            <tui-icon icon="@tui.save" />
            {{ isEditing() ? 'Update' : 'Create' }}
          </button>
        </div>
      }
    </app-page-container>
  `,
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
