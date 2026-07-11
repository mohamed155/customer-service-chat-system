import { ChangeDetectionStrategy, Component, computed, effect, inject } from '@angular/core';
import { EmptyStateComponent } from '../../../shared/components/empty-state/empty-state.component';
import { LoadingStateComponent } from '../../../shared/components/loading-state/loading-state.component';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { CustomerPanelComponent } from './customer-panel.component';
import { ConversationThreadComponent } from './conversation-thread.component';
import { ConversationsStore } from './conversations.store';
import { InboxListComponent } from './inbox-list.component';
import { PAGE_ROUTE, RoutedPageStore } from '../routed-page.store';

@Component({
  selector: 'app-conversations',
  imports: [
    ConversationThreadComponent,
    CustomerPanelComponent,
    EmptyStateComponent,
    InboxListComponent,
    LoadingStateComponent,
    PageContainerComponent,
    PageHeaderComponent,
  ],
  providers: [
    ConversationsStore,
    RoutedPageStore,
    { provide: PAGE_ROUTE, useValue: 'conversations' },
  ],
  template: `
    <app-page-container>
      <app-page-header title="Conversations" [description]="'Shared inbox · 6 open, 2 escalated'" />
      @if (page.loading()) {
        <app-loading-state />
      } @else if (hasError()) {
        <app-empty-state
          icon="@tui.alert-circle"
          title="Something went wrong"
          description="We couldn't load this page. Please try again."
        >
          <button type="button" (click)="retry()">Try again</button>
        </app-empty-state>
      } @else if (store.filteredConversations().length) {
        <section class="inbox-shell">
          <app-inbox-list
            [conversations]="store.filteredConversations()"
            [customers]="store.customers()"
            [selectedId]="store.selectedId()"
            [statusFilter]="store.statusFilter()"
            (selected)="store.select($event)"
            (filterChanged)="store.setFilter($event)"
          />
          <app-conversation-thread
            [conversation]="store.selectedConversation()"
            [customer]="store.selectedCustomer()"
          />
          <app-customer-panel [customer]="store.selectedCustomer()" />
        </section>
      } @else if (store.statusFilter() !== 'all') {
        <app-empty-state
          icon="@tui.message-circle"
          title="No conversations match"
          description="Try adjusting the status filter to find what you're looking for."
        >
          <button type="button" (click)="store.setFilter('all')">Show all conversations</button>
        </app-empty-state>
      } @else {
        <app-empty-state
          icon="@tui.message-circle"
          title="No conversations yet"
          description="Customer conversations will appear here once your customers start reaching out."
        >
        </app-empty-state>
      }
    </app-page-container>
  `,
  styles: [
    `
      .inbox-shell {
        height: calc(100dvh - var(--app-topbar-height) - (var(--app-page-padding-y) * 2));
        min-height: 680px;
        display: grid;
        grid-template-columns: minmax(320px, 380px) minmax(0, 1fr) 300px;
        overflow: hidden;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-xl);
        background: var(--app-panel);
        box-shadow: var(--app-shadow);
      }
      @media (max-width: 1024px) {
        .inbox-shell {
          grid-template-columns: minmax(300px, 360px) minmax(0, 1fr);
        }
      }
      @media (max-width: 768px) {
        .inbox-shell {
          height: auto;
          min-height: 0;
          grid-template-columns: 1fr;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ConversationsComponent {
  protected readonly page = inject(RoutedPageStore);
  protected readonly hasError = computed(() => this.page.error() !== null);
  protected readonly store = inject(ConversationsStore);

  constructor() {
    effect(() => {
      const data = this.page.data();
      if (data?.page === 'conversations') {
        this.store.setPageData(data.data.conversations, data.data.customers);
      }
    });
  }

  protected retry(): void {
    this.page.retry();
  }
}
