import { TestBed } from '@angular/core/testing';
import { ConversationsApiService } from './conversations-api.service';
import { ConversationsStore } from './conversations.store';

describe('ConversationsStore', () => {
  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        ConversationsStore,
        {
          provide: ConversationsApiService,
          useValue: {
            list: vi.fn(),
            getFeedbackSummary: vi.fn(),
          },
        },
      ],
    });
  });

  it('starts with empty state', () => {
    const store = TestBed.inject(ConversationsStore);
    expect(store.items()).toEqual([]);
    expect(store.selectedId()).toBeNull();
    expect(store.loading()).toBe(false);
    expect(store.error()).toBeNull();
    expect(store.filters()).toEqual({ status: 'open' });
  });

  it('selects a conversation by id', () => {
    const store = TestBed.inject(ConversationsStore);
    TestBed.inject(ConversationsApiService).list = vi.fn().mockReturnValue({
      pipe: () => ({
        pipe: vi.fn(),
      }),
    });

    store.select('c2');
    expect(store.selectedId()).toBe('c2');
    expect(store.selectedConversation()).toBeNull(); // no items yet
  });
});
