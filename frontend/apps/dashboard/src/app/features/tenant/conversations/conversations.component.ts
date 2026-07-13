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

@Component({
  selector: 'app-conversations',
  imports: [
    EmptyStateComponent,
    InboxListComponent,
    LoadingStateComponent,
    NewConversationDialogComponent,
    PageContainerComponent,
    PageHeaderComponent,
  ],
  providers: [ConversationsStore],
  template: `
    <app-page-container>
      <app-page-header title="Conversations">
        @if (hasManagePerm()) {
          <button type="button" class="new-btn" (click)="showNewDialog.set(true)">
            New conversation
          </button>
        }
      </app-page-header>

      @if (store.loading() && !store.items().length) {
        <app-loading-state />
      } @else if (store.error(); as err) {
        <app-empty-state
          icon="@tui.alert-circle"
          title="Something went wrong"
          description="We couldn't load conversations. Please try again."
        >
          <button type="button" (click)="store.loadInbox()">Try again</button>
        </app-empty-state>
      } @else if (store.items().length) {
        <div class="filter-bar">
          <select
            class="filter-select"
            [value]="store.filters().channel ?? ''"
            (change)="onChannelChange($event)"
          >
            <option value="">All channels</option>
            <option value="email">Email</option>
            <option value="web_chat">Web Chat</option>
            <option value="whatsapp">WhatsApp</option>
            <option value="telegram">Telegram</option>
            <option value="phone">Phone</option>
          </select>
          <select
            class="filter-select"
            [value]="store.filters().assignee ?? ''"
            (change)="onAssigneeChange($event)"
          >
            <option value="">All assignees</option>
            <option value="me">Me</option>
            <option value="unassigned">Unassigned</option>
            @for (member of members(); track member.id) {
              <option [value]="member.id">{{ member.displayName }}</option>
            }
          </select>
        </div>

        <section class="inbox-shell">
          <app-inbox-list
            [conversations]="store.items()"
            [selectedId]="store.selectedId()"
            [statusFilter]="store.statusFilter()"
            (selected)="onSelectConversation($event)"
            (filterChanged)="store.setFilter({ status: $event })"
          />
        </section>

        @if (store.hasMore()) {
          <div class="pagination">
            <button type="button" class="load-more" (click)="store.nextPage()">Load more</button>
          </div>
        }
      } @else if (store.statusFilter() !== 'all') {
        <app-empty-state
          icon="@tui.message-circle"
          title="No conversations match"
          description="Try adjusting the filters to find what you're looking for."
        >
          <button type="button" (click)="store.resetFilters()">Show all conversations</button>
        </app-empty-state>
      } @else {
        <app-empty-state
          icon="@tui.message-circle"
          title="No conversations yet"
          description="Customer conversations will appear here once your customers start reaching out."
        />
      }

      @if (showNewDialog()) {
        <app-new-conversation-dialog
          (create)="onConversationCreated($event)"
          (closeDialog)="showNewDialog.set(false)"
        />
      }
    </app-page-container>
  `,
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
    this.router.navigate(['/', APP_PATHS.tenant.base, APP_PATHS.tenant.conversationDetail(id)]);
  }

  protected onConversationCreated(id: string): void {
    this.showNewDialog.set(false);
    this.router.navigate(['/', APP_PATHS.tenant.base, APP_PATHS.tenant.conversationDetail(id)]);
  }

  protected onChannelChange(event: Event): void {
    const value = (event.target as HTMLSelectElement).value;
    this.store.setFilter({ channel: value || undefined });
  }

  protected onAssigneeChange(event: Event): void {
    const value = (event.target as HTMLSelectElement).value;
    this.store.setFilter({ assignee: value || undefined });
  }
}
