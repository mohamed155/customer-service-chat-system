import { computed } from '@angular/core';
import { patchState, signalStore, withComputed, withMethods, withState } from '@ngrx/signals';
import {
  ConversationFixture,
  ConversationStatus,
  CustomerFixture,
} from '../../../shared/fixtures/fixture.models';

export type ConversationStatusFilter = ConversationStatus | 'all';

interface ConversationsState {
  readonly conversations: ConversationFixture[];
  readonly customers: CustomerFixture[];
  readonly selectedId: string | null;
  readonly statusFilter: ConversationStatusFilter;
}

export const ConversationsStore = signalStore(
  withState<ConversationsState>({
    conversations: [],
    customers: [],
    selectedId: null,
    statusFilter: 'all',
  }),
  withComputed(({ conversations, customers, selectedId, statusFilter }) => ({
    filteredConversations: computed(() =>
      statusFilter() === 'all'
        ? conversations()
        : conversations().filter((conversation) => conversation.status === statusFilter()),
    ),
    selectedConversation: computed(
      () => conversations().find((conversation) => conversation.id === selectedId()) ?? null,
    ),
    selectedCustomer: computed(() => {
      const selected = conversations().find((conversation) => conversation.id === selectedId());
      return selected
        ? (customers().find((customer) => customer.id === selected.customerId) ?? null)
        : null;
    }),
  })),
  withMethods((store) => ({
    setPageData(
      conversations: readonly ConversationFixture[],
      customers: readonly CustomerFixture[],
    ): void {
      patchState(store, {
        conversations: [...conversations],
        customers: [...customers],
        selectedId: conversations[0]?.id ?? null,
        statusFilter: 'all',
      });
    },
    select(id: string): void {
      patchState(store, { selectedId: id });
    },
    setFilter(statusFilter: ConversationStatusFilter): void {
      const visible =
        statusFilter === 'all'
          ? store.conversations()
          : store.conversations().filter((conversation) => conversation.status === statusFilter);
      const selectionStillVisible = visible.some(
        (conversation) => conversation.id === store.selectedId(),
      );
      patchState(store, {
        statusFilter,
        selectedId: selectionStillVisible ? store.selectedId() : (visible[0]?.id ?? null),
      });
    },
  })),
);
