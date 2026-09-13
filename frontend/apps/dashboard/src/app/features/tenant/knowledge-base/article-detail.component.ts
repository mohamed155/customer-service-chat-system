import { DatePipe } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { TuiIcon } from '@taiga-ui/core';
import { APP_PATHS } from '../../../core/router/app-paths';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { DashboardCardComponent } from '../../../shared/components/dashboard-card/dashboard-card.component';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { StatusBadgeComponent } from '../../../shared/components/status-badge/status-badge.component';
import { IndexStatusBadgeComponent } from './index-status-badge.component';
import { KnowledgeStore } from './knowledge.store';

@Component({
  selector: 'app-article-detail',
  imports: [
    DashboardCardComponent,
    DatePipe,
    EmptyStateComponent,
    IndexStatusBadgeComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
    RouterLink,
    StatusBadgeComponent,
    TuiIcon,
  ],
  providers: [KnowledgeStore],
  templateUrl: './article-detail.component.html',
  styles: [
    `
      .action-btn,
      .edit-link {
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
        text-decoration: none;
        cursor: pointer;
      }
      .action-btn:hover,
      .edit-link:hover {
        background: var(--app-panel-2);
      }
      .action-btn.danger {
        border-color: var(--app-danger);
        color: var(--app-danger);
      }
      .action-btn.danger:hover {
        background: var(--app-danger-soft);
      }
      .action-btn:disabled {
        opacity: 0.6;
        cursor: default;
      }
      .detail-grid {
        display: grid;
        gap: var(--app-space-4);
      }
      .meta-grid {
        display: grid;
        gap: var(--app-space-3);
      }
      .meta-item {
        display: flex;
        align-items: center;
        gap: var(--app-space-2);
        font-size: var(--app-font-sm);
      }
      .meta-label {
        color: var(--app-text-2);
        font-weight: 650;
        min-width: 80px;
      }
      .meta-value {
        color: var(--app-text);
      }
      .tags {
        display: flex;
        gap: var(--app-space-1);
        flex-wrap: wrap;
      }
      .tag {
        padding: 2px 8px;
        border-radius: 999px;
        background: var(--app-panel-2);
        color: var(--app-text-2);
        font-size: var(--app-font-xs);
        font-weight: 600;
      }
      .body {
        line-height: 1.7;
        color: var(--app-text);
      }
      .body ::ng-deep h1,
      .body ::ng-deep h2,
      .body ::ng-deep h3 {
        margin: 1em 0 0.5em;
      }
      .body ::ng-deep p {
        margin: 0 0 1em;
      }
      .body ::ng-deep ul,
      .body ::ng-deep ol {
        margin: 0 0 1em;
        padding-left: 1.5em;
      }
      .body ::ng-deep a {
        color: var(--app-accent-strong);
        text-decoration: underline;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ArticleDetailComponent {
  readonly store = inject(KnowledgeStore);
  private readonly route = inject(ActivatedRoute);
  private readonly permissions = inject(PermissionsService);

  protected readonly APP_PATHS = APP_PATHS;

  protected readonly item = computed(() => this.store.selectedItem());
  protected readonly editPath = computed(() => {
    const id = this.item()?.id;
    return id ? APP_PATHS.tenant.knowledgeBaseEdit(id) : '';
  });
  protected readonly canManage = computed(() => this.permissions.has('knowledge_base.manage'));

  constructor() {
    const id = this.route.snapshot.paramMap.get('id');
    if (id) {
      this.store.loadItem(id);
    }
  }

  protected publish(): void {
    const id = this.item()?.id;
    if (id) this.store.setStatus(id, { status: 'published' });
  }

  protected archive(): void {
    const id = this.item()?.id;
    if (id) this.store.setStatus(id, { status: 'archived' });
  }

  protected restore(): void {
    const id = this.item()?.id;
    if (id) this.store.setStatus(id, { status: 'draft' });
  }

  protected reindex(): void {
    const id = this.item()?.id;
    if (id) this.store.reindex(id);
  }

  protected indexTooltip(status: { status: string; failureReason?: string }): string {
    if (status.status === 'failed' && status.failureReason) return status.failureReason;
    if (status.status === 'not_indexable' && status.failureReason) return status.failureReason;
    return '';
  }

  protected typeLabel(type: string): string {
    return type === 'faq' ? 'FAQ' : type.charAt(0).toUpperCase() + type.slice(1);
  }

  protected statusTone(status: string): 'green' | 'amber' | 'red' {
    return status === 'published' ? 'green' : status === 'draft' ? 'amber' : 'red';
  }
}
