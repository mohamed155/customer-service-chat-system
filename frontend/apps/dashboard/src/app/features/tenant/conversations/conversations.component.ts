import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { Router } from '@angular/router';
import { map } from 'rxjs';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { APP_PATHS } from '../../../core/router/app-paths';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { ConversationsApiService } from './conversations-api.service';
import { ConversationsStore } from './conversations.store';
import { InboxListComponent } from './inbox-list.component';
import { NewConversationDialogComponent } from './new-conversation-dialog.component';
import { SatisfactionSummaryComponent } from './satisfaction-summary.component';
import { ApiError } from '../../../core/api/api.models';
import { errorMessageKey } from '../../../core/errors/error-message';
import { ErrorReferenceComponent } from '../../../shared/components/error-reference/error-reference.component';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

@Component({
  selector: 'app-conversations',
  imports: [
    EmptyStateComponent,
    ErrorReferenceComponent,
    InboxListComponent,
    LoadingStateComponent,
    NewConversationDialogComponent,
    PageContainerComponent,
    PageHeaderComponent,
    SatisfactionSummaryComponent,
    TranslocoPipe,
  ],
  providers: [provideTranslocoScope('conversations'), ConversationsStore],
  templateUrl: './conversations.component.html',
  styles: [
    `
      .inbox-shell {
        display: block;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-xl);
        background: var(--app-panel);
        box-shadow: var(--app-shadow);
      }
      .filter-bar {
        display: flex;
        gap: var(--app-space-2);
        margin-bottom: var(--app-space-3);
      }
      .filter-select {
        height: 34px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font-size: var(--app-font-sm);
      }
      .filter-chip {
        height: 34px;
        padding: 0 var(--app-space-3);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
        font-weight: 600;
        cursor: pointer;
      }
      .filter-chip.active {
        background: var(--app-accent);
        color: var(--app-accent-ink);
        border-color: var(--app-accent);
      }
      .new-btn {
        height: 34px;
        padding: 0 var(--app-space-4);
        border: 1px solid var(--app-accent);
        border-radius: var(--app-radius-md);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        font-weight: 600;
        cursor: pointer;
      }
      .pagination {
        display: flex;
        justify-content: center;
        margin-top: var(--app-space-4);
      }
      .load-more {
        height: 36px;
        padding: 0 var(--app-space-5);
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text);
        font-weight: 600;
        cursor: pointer;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ConversationsComponent {
  protected errorKey(error: ApiError): string {
    return errorMessageKey(error);
  }

  protected readonly store = inject(ConversationsStore);
  private readonly api = inject(ConversationsApiService);
  private readonly router = inject(Router);
  private readonly permissions = inject(PermissionsService);
  protected readonly members = toSignal(this.api.listAssignableMembers().pipe(map((r) => r.data)), {
    initialValue: [],
  });
  protected readonly showNewDialog = signal(false);

  protected readonly hasManagePerm = () => this.permissions.has('conversations.manage' as const);

  protected onSelectConversation(id: string): void {
    this.router.navigateByUrl(
      `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.conversationDetail(id)}`,
    );
  }

  protected onConversationCreated(id: string): void {
    this.showNewDialog.set(false);
    this.router.navigateByUrl(
      `/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.conversationDetail(id)}`,
    );
  }

  protected onChannelChange(event: Event): void {
    const value = (event.target as HTMLSelectElement).value;
    this.store.setFilter({ channel: value || undefined });
  }

  protected onAssigneeChange(event: Event): void {
    const value = (event.target as HTMLSelectElement).value;
    this.store.setFilter({ assignee: value || undefined });
  }

  protected onEscalatedToggle(): void {
    const current = this.store.filters().escalated;
    this.store.setFilter({ escalated: current === 'true' ? undefined : 'true' });
  }
}
