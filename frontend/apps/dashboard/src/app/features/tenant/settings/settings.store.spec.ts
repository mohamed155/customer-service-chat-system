import { TestBed } from '@angular/core/testing';
import { SettingsStore } from './settings.store';

describe('SettingsStore', () => {
  beforeEach(() => TestBed.configureTestingModule({ providers: [SettingsStore] }));

  it('initializes and updates the active tab', () => {
    const store = TestBed.inject(SettingsStore);

    expect(store.activeTab()).toBe('general');
    store.setTab('security');
    expect(store.activeTab()).toBe('security');
  });
});
