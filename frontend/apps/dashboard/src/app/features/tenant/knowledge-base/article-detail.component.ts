import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { TuiIcon } from '@taiga-ui/core';
import { ApiError } from '../../../core/api/api.models';
import { ErrorKeyOverrides, errorMessageKey } from '../../../core/errors/error-message';
import { marker } from '../../../core/i18n/marker';
import { LocaleDatePipe } from '../../../core/i18n/format';
import { LocaleService } from '../../../core/i18n/locale.service';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { statusKey, statusTone, typeKey } from './knowledge-labels';
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

/**
 * The status buttons only offer a change the rules allow, so the API's `422 validation_failed`
 * (it answers an illegal change and a publish with no content with the same code) means the
 * article has no content to publish.
 */
const DETAIL_ERRORS: ErrorKeyOverrides = {
  422: marker('knowledge-base.detail.publishNeedsContent'),
};

@Component({
  selector: 'app-article-detail',
  imports: [
    DashboardCardComponent,
    EmptyStateComponent,
    ErrorReferenceComponent,
    IndexStatusBadgeComponent,
    LoadingStateComponent,
    LocaleDatePipe,
    PageContainerComponent,
    PageHeaderComponent,
    RouterLink,
    StatusBadgeComponent,
    TranslocoPipe,
    TuiIcon,
  ],
  providers: [KnowledgeStore, provideTranslocoScope('knowledge-base')],
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
  private readonly locale = inject(LocaleService);

  protected readonly APP_PATHS = APP_PATHS;
  protected readonly statusKey = statusKey;
  protected readonly typeKey = typeKey;
  protected readonly statusTone = statusTone;

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

  /** The name from the catalog, or the API's own word for a value this app has no name for. */
  protected nameOf(key: string | null, fallback: string): string {
    return key ? this.locale.translate(key) : fallback;
  }

  protected errorKey(error: ApiError): string {
    return errorMessageKey(error, DETAIL_ERRORS);
  }
}
