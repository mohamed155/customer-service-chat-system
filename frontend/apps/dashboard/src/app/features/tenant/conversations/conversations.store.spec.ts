import { TestBed } from '@angular/core/testing';
import { CONVERSATION_FIXTURES } from '../../../shared/fixtures/conversation.fixtures';
import { CUSTOMER_FIXTURES } from '../../../shared/fixtures/customer.fixtures';
import { ConversationsStore } from './conversations.store';

describe('ConversationsStore', () => {
  beforeEach(() => {
    TestBed.configureTestingModule({ providers: [ConversationsStore] });
  });

  it('starts with empty state until setPageData is called', () => {
    const store = TestBed.inject(ConversationsStore);

    expect(store.conversations()).toEqual([]);
    expect(store.selectedId()).toBeNull();
  });

  it('setPageData populates conversations and selects the first one', () => {
    const store = TestBed.inject(ConversationsStore);
    store.setPageData(CONVERSATION_FIXTURES, CUSTOMER_FIXTURES);

    expect(store.selectedId()).toBe(CONVERSATION_FIXTURES[0].id);
    expect(store.selectedConversation()?.id).toBe(CONVERSATION_FIXTURES[0].id);
  });

  it('updates selection and moves hidden selection when filtering', () => {
    const store = TestBed.inject(ConversationsStore);
    store.setPageData(CONVERSATION_FIXTURES, CUSTOMER_FIXTURES);
    const closed = CONVERSATION_FIXTURES.find((conversation) => conversation.status === 'closed')!;
    store.select(closed.id);
    expect(store.selectedId()).toBe(closed.id);

    store.setFilter('open');

    expect(
      store.filteredConversations().every((conversation) => conversation.status === 'open'),
    ).toBe(true);
    expect(store.selectedConversation()?.status).toBe('open');
  });
});
