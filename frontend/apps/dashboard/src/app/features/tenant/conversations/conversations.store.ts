import { computed } from '@angular/core';
import { patchState, signalStore, withComputed, withMethods, withState } from '@ngrx/signals';
import { CONVERSATION_FIXTURES } from '../../../shared/fixtures/conversation.fixtures';
import { CUSTOMER_FIXTURES } from '../../../shared/fixtures/customer.fixtures';
import { ConversationStatus } from '../../../shared/fixtures/fixture.models';

export type ConversationStatusFilter = ConversationStatus | 'all';

interface ConversationsState {
  readonly selectedId: string | null;
  readonly statusFilter: ConversationStatusFilter;
}

const firstConversationId = CONVERSATION_FIXTURES[0]?.id ?? null;

export const ConversationsStore = signalStore(
  withState<ConversationsState>({
    selectedId: firstConversationId,
    statusFilter: 'all',
  }),
  withComputed(({ selectedId, statusFilter }) => ({
    filteredConversations: computed(() =>
      statusFilter() === 'all'
        ? CONVERSATION_FIXTURES
        : CONVERSATION_FIXTURES.filter((conversation) => conversation.status === statusFilter()),
    ),
    selectedConversation: computed(
      () => CONVERSATION_FIXTURES.find((conversation) => conversation.id === selectedId()) ?? null,
    ),
    selectedCustomer: computed(() => {
      const selected = CONVERSATION_FIXTURES.find(
        (conversation) => conversation.id === selectedId(),
      );
      return selected
        ? (CUSTOMER_FIXTURES.find((customer) => customer.id === selected.customerId) ?? null)
        : null;
    }),
  })),
  withMethods((store) => ({
    select(id: string): void {
      patchState(store, { selectedId: id });
    },
    setFilter(statusFilter: ConversationStatusFilter): void {
      const visible =
        statusFilter === 'all'
          ? CONVERSATION_FIXTURES
          : CONVERSATION_FIXTURES.filter((conversation) => conversation.status === statusFilter);
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
