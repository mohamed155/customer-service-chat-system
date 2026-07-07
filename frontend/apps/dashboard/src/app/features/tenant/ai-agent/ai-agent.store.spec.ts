import { TestBed } from '@angular/core/testing';
import { AiAgentStore } from './ai-agent.store';

describe('AiAgentStore', () => {
  beforeEach(() => TestBed.configureTestingModule({ providers: [AiAgentStore] }));

  it('initializes and updates the active tab', () => {
    const store = TestBed.inject(AiAgentStore);

    expect(store.activeTab()).toBe('behavior');
    store.setTab('testing');
    expect(store.activeTab()).toBe('testing');
  });
});
