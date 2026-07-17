import { DatePipe } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { TuiIcon } from '@taiga-ui/core';
import { KnowledgeItemStatus, KnowledgeItemType } from '../../../core/api/knowledge.models';
import { APP_PATHS } from '../../../core/router/app-paths';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { SearchInputComponent } from '../../../shared/components/search-input/search-input.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { ToolbarComponent } from '../../../shared/components/toolbar/toolbar.component';
import { CategoryManagerComponent } from './category-manager.component';
import { KnowledgeStore } from './knowledge.store';
import { UploadDocumentComponent } from './upload-document.component';

@Component({
  selector: 'app-knowledge-base',
  imports: [
    CategoryManagerComponent,
    DashboardCardComponent,
    DatePipe,
    EmptyStateComponent,
    FormsModule,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    RouterLink,
    SearchInputComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
    ToolbarComponent,
    TuiIcon,
    UploadDocumentComponent,
  ],
  providers: [KnowledgeStore],
  template: `
    <app-page-container>
      <app-page-header
        title="Knowledge Base"
        description="Train your AI with trusted company knowledge"
      >
        @if (canManage()) {
          <a class="new-btn" [routerLink]="APP_PATHS.tenant.knowledgeBaseNew">
            <tui-icon icon="@tui.plus" />
            New article
          </a>
        }
      </app-page-header>

      @if (store.loading() && !store.items().length) {
        <app-loading-state />
      } @else if (store.error(); as err) {
        <app-empty-state icon="@tui.alert-circle" title="Something went wrong" [description]="err">
          <button type="button" (click)="store.loadList()">Try again</button>
        </app-empty-state>
      } @else {
        <app-section-header title="Knowledge articles" subtitle="Trusted sources for AI answers" />
        <div class="stack">
          <app-toolbar>
            <app-search-input toolbar-start placeholder="Search knowledge" [(value)]="query" />
            @if (canManage()) {
              <button toolbar-end type="button" class="upload-btn" (click)="uploadOpen.set(true)">
                <tui-icon icon="@tui.upload" />
                Upload
              </button>
            }
            <select
              toolbar-end
              aria-label="Status filter"
              [ngModel]="statusFilter()"
              (ngModelChange)="statusFilter.set($event); onFilterChange()"
            >
              <option value="">All statuses</option>
              <option value="draft">Draft</option>
              <option value="published">Published</option>
              <option value="archived">Archived</option>
            </select>
            <select
              toolbar-end
              aria-label="Category filter"
              [ngModel]="categoryFilter()"
              (ngModelChange)="categoryFilter.set($event); onFilterChange()"
            >
              <option value="">All categories</option>
              @for (cat of store.categories(); track cat.id) {
                <option [value]="cat.id">{{ cat.name }}</option>
              }
            </select>
            <input
              toolbar-end
              type="text"
              aria-label="Tag filter"
              placeholder="Filter by tag"
              [ngModel]="tagFilter()"
              (ngModelChange)="tagFilter.set($event); onFilterChange()"
            />
            @if (canManage()) {
              <button
                toolbar-end
                type="button"
                class="manage-cats-btn"
                (click)="showCategoryManager.set(true)"
              >
                <tui-icon icon="@tui.settings" />
                Categories
              </button>
            }
          </app-toolbar>

          @if (items().length) {
            <section class="cards">
              @for (article of items(); track article.id) {
                <app-dashboard-card>
                  <div class="article-head">
                    <div>
                      <strong>{{ article.title }}</strong>
                      <span>
                        {{ article.categoryName ?? 'Uncategorized' }}
                        · Updated {{ article.updatedAt | date: 'mediumDate' }}
                      </span>
                    </div>
                    <a
                      class="detail-link"
                      [routerLink]="APP_PATHS.tenant.knowledgeBaseDetail(article.id)"
                    >
                      <tui-icon icon="@tui.eye" />
                    </a>
                  </div>
                  <div class="badges">
                    <app-status-badge
                      [status]="article.status"
                      [tone]="statusTone(article.status)"
                    />
                    <app-status-badge [status]="typeLabel(article.itemType)" tone="neutral" />
                    @if (article.tags.length) {
                      <span class="tag-count">{{ article.tags.length }} tag(s)</span>
                    }
                  </div>
                  @if (canManage()) {
                    <div class="card-actions" card-footer>
                      <a
                        class="action-link"
                        [routerLink]="APP_PATHS.tenant.knowledgeBaseEdit(article.id)"
                      >
                        <tui-icon icon="@tui.edit" />
                        Edit
                      </a>
                    </div>
                  }
                </app-dashboard-card>
              }
            </section>

            @if (store.hasMore()) {
              <div class="load-more">
                <button type="button" [disabled]="store.loading()" (click)="store.loadMore()">
                  @if (store.loading()) {
                    Loading…
                  } @else {
                    Load more
                  }
                </button>
              </div>
            }
          } @else if (hasData()) {
            <app-empty-state
              icon="@tui.search-x"
              title="No articles match"
              description="Adjust the search or category filter to show knowledge sources."
            />
          } @else {
            <app-empty-state
              icon="@tui.book-open"
              title="No articles yet"
              description="Create your first knowledge article to train the AI assistant."
            />
          }
        </div>
      }
    </app-page-container>

    <app-upload-document
      [open]="uploadOpen()"
      (completed)="onUploaded()"
      (closed)="uploadOpen.set(false)"
    />
  `,
  styles: [
    `
      .new-btn {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-2);
        height: 38px;
        padding: 0 var(--app-space-4);
        border: 1px solid var(--app-accent);
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 650;
        font-size: var(--app-font-sm);
        text-decoration: none;
        cursor: pointer;
      }
      .new-btn:hover {
        opacity: 0.92;
      }
      .upload-btn {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-2);
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
      .upload-btn:hover {
        background: var(--app-panel-2);
      }
      select {
        height: 38px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
        font-weight: 650;
      }
      .stack {
        display: grid;
        gap: var(--app-space-4);
      }
      .cards {
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: var(--app-space-4);
      }
      .article-head {
        display: flex;
        justify-content: space-between;
        gap: var(--app-space-3);
      }
      strong {
        display: block;
        color: var(--app-text);
      }
      .article-head span {
        display: block;
        margin-top: 4px;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .detail-link {
        display: grid;
        place-items: center;
        width: 32px;
        height: 32px;
        border-radius: var(--app-radius-md);
        color: var(--app-text-2);
        text-decoration: none;
        flex-shrink: 0;
      }
      .detail-link:hover {
        background: var(--app-panel-2);
        color: var(--app-text);
      }
      .badges {
        display: flex;
        gap: var(--app-space-2);
        flex-wrap: wrap;
        align-items: center;
      }
      .tag-count {
        font-size: var(--app-font-xs);
        color: var(--app-text-3);
      }
      .card-actions {
        display: flex;
        gap: var(--app-space-2);
      }
      .action-link {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-1);
        height: 30px;
        padding: 0 var(--app-space-3);
        border-radius: var(--app-radius-md);
        background: var(--app-panel-2);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        font-weight: 650;
        text-decoration: none;
      }
      .action-link:hover {
        background: var(--app-accent-soft);
        color: var(--app-accent-strong);
      }
      .load-more {
        display: flex;
        justify-content: center;
      }
      .load-more button {
        height: 38px;
        padding: 0 var(--app-space-6);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font-weight: 650;
        cursor: pointer;
      }
      .load-more button:disabled {
        opacity: 0.6;
        cursor: default;
      }
      @media (max-width: 1100px) {
        .cards {
          grid-template-columns: repeat(2, minmax(0, 1fr));
        }
      }
      @media (max-width: 768px) {
        .cards {
          grid-template-columns: 1fr;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class KnowledgeBaseComponent {
  readonly store = inject(KnowledgeStore);
  private readonly permissions = inject(PermissionsService);
  protected readonly APP_PATHS = APP_PATHS;

  protected readonly query = signal('');
  protected readonly categoryFilter = signal('');
  protected readonly statusFilter = signal('');
  protected readonly tagFilter = signal('');
  protected readonly showCategoryManager = signal(false);

  protected readonly canManage = computed(() => this.permissions.has('knowledge_base.manage'));

  protected readonly items = computed(() => {
    const q = this.query().trim().toLowerCase();
    const cat = this.categoryFilter();
    return this.store.items().filter((item) => {
      const matchesQuery = q
        ? `${item.title} ${item.categoryName ?? ''}`.toLowerCase().includes(q)
        : true;
      const matchesCategory = !cat || item.categoryId === cat;
      return matchesQuery && matchesCategory;
    });
  });

  protected readonly hasData = computed(() => this.store.items().length > 0);

  protected readonly uploadOpen = signal(false);

  protected onUploaded(): void {
    this.uploadOpen.set(false);
    this.store.loadList();
  }

  protected onFilterChange(): void {
    this.store.setFilter({
      categoryId: this.categoryFilter() || undefined,
      status: (this.statusFilter() || undefined) as KnowledgeItemStatus | undefined,
      tag: this.tagFilter() || undefined,
    });
  }

  protected statusTone(status: KnowledgeItemStatus): 'green' | 'amber' | 'red' {
    return status === 'published' ? 'green' : status === 'draft' ? 'amber' : 'red';
  }

  protected typeLabel(type: KnowledgeItemType): string {
    return type === 'faq' ? 'FAQ' : type === 'document' ? 'Document' : 'Article';
  }
}
