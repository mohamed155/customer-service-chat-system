import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { PageContainerComponent } from '../../../layout/page-container/page-container.component';
import { PageHeaderComponent } from '../../../layout/page-header/page-header.component';
import { CustomerPanelComponent } from './customer-panel.component';
import { ConversationThreadComponent } from './conversation-thread.component';
import { ConversationsStore } from './conversations.store';
import { InboxListComponent } from './inbox-list.component';

@Component({
  selector: 'app-conversations',
  imports: [
    ConversationThreadComponent,
    CustomerPanelComponent,
    InboxListComponent,
    PageContainerComponent,
    PageHeaderComponent,
  ],
  providers: [ConversationsStore],
  template: `
    <app-page-container>
      <app-page-header title="Conversations" [description]="'Shared inbox · 6 open, 2 escalated'" />
      <section class="inbox-shell">
        <app-inbox-list
          [conversations]="store.filteredConversations()"
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
  protected readonly store = inject(ConversationsStore);
}
