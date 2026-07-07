import { ChangeDetectionStrategy, Component, computed, signal } from '@angular/core';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { SearchInputComponent } from '../../../shared/components/search-input/search-input.component';
import { SectionHeaderComponent } from '../../../shared/components/section-header/section-header.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { ToolbarComponent } from '../../../shared/components/toolbar/toolbar.component';
import { KNOWLEDGE_FIXTURES } from '../../../shared/fixtures/knowledge.fixtures';
import { ArticleStatus, ArticleSource } from '../../../shared/fixtures/fixture.models';

@Component({
  selector: 'app-knowledge-base',
  imports: [
    DashboardCardComponent,
    EmptyStateComponent,
    PageContainerComponent,
    SearchInputComponent,
    SectionHeaderComponent,
    StatusBadgeComponent,
    ToolbarComponent,
  ],
  template: `
    <app-page-container>
      <app-section-header title="Knowledge articles" subtitle="Trusted sources for AI answers">
        <button type="button">New article</button>
      </app-section-header>
      <div class="stack">
        <app-toolbar>
          <app-search-input toolbar-start placeholder="Search knowledge" [(value)]="query" />
          <select
            toolbar-end
            aria-label="Category filter"
            [value]="category()"
            (change)="setCategory($event)"
          >
            <option value="all">All categories</option>
            @for (item of categories(); track item) {
              <option [value]="item">{{ item }}</option>
            }
          </select>
        </app-toolbar>

        @if (articles().length) {
          <section class="cards">
            @for (article of articles(); track article.id) {
              <app-dashboard-card>
                <div class="article-head">
                  <div>
                    <strong>{{ article.title }}</strong>
                    <span>{{ article.category }} · Updated {{ article.updatedAt }}</span>
                  </div>
                  <span class="indexed" [class.off]="!article.indexed">
                    {{ article.indexed ? 'Indexed' : 'Re-index needed' }}
                  </span>
                </div>
                <p>{{ article.excerpt }}</p>
                <div class="badges">
                  <app-status-badge [status]="article.status" [tone]="statusTone(article.status)" />
                  <app-status-badge [status]="sourceLabel(article.source)" tone="neutral" />
                </div>
              </app-dashboard-card>
            }
          </section>
        } @else {
          <app-empty-state
            icon="@tui.search-x"
            title="No articles match"
            description="Adjust the search or category filter to show knowledge sources."
          />
        }
      </div>
    </app-page-container>
  `,
  styles: [
    `
      button,
      select {
        height: 38px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        padding: 0 var(--app-space-3);
        font-weight: 650;
      }
      app-section-header button {
        border-color: var(--app-accent);
        background: var(--app-accent);
        color: var(--app-accent-ink);
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
      .article-head span:not(.indexed) {
        display: block;
        margin-top: 4px;
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .indexed {
        height: fit-content;
        padding: 4px 8px;
        border-radius: 999px;
        background: var(--app-green-soft);
        color: var(--app-green);
        font-size: var(--app-font-xs);
        font-weight: 700;
        white-space: nowrap;
      }
      .indexed.off {
        background: var(--app-amber-soft);
        color: var(--app-amber);
      }
      p {
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        line-height: 1.5;
      }
      .badges {
        display: flex;
        gap: var(--app-space-2);
        flex-wrap: wrap;
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
  protected readonly query = signal('');
  protected readonly category = signal('all');
  protected readonly categories = computed(() => [
    ...new Set(KNOWLEDGE_FIXTURES.map((article) => article.category)),
  ]);
  protected readonly articles = computed(() => {
    const query = this.query().trim().toLowerCase();
    const category = this.category();
    return KNOWLEDGE_FIXTURES.filter((article) => {
      const matchesQuery = query
        ? `${article.title} ${article.excerpt} ${article.category}`.toLowerCase().includes(query)
        : true;
      const matchesCategory = category === 'all' || article.category === category;
      return matchesQuery && matchesCategory;
    });
  });

  protected setCategory(event: Event): void {
    this.category.set((event.target as HTMLSelectElement).value);
  }

  protected statusTone(status: ArticleStatus): 'green' | 'amber' | 'red' {
    return status === 'published' ? 'green' : status === 'draft' ? 'amber' : 'red';
  }

  protected sourceLabel(source: ArticleSource): string {
    return source === 'faq' ? 'FAQ' : source.toUpperCase();
  }
}
