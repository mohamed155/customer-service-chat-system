import { TestBed } from '@angular/core/testing';
import { provideMockStore, MockStore } from '@ngrx/store/testing';
import { appUiActions } from '../../core/state/app-ui.feature';
import { LayoutStore } from './layout.store';

describe('LayoutStore', () => {
  it('collapses globally when initialized in a narrow viewport', () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 800 });
    TestBed.configureTestingModule({ providers: [LayoutStore, provideMockStore()] });
    const globalStore = TestBed.inject(MockStore);
    const dispatch = vi.spyOn(globalStore, 'dispatch');
    const layout = TestBed.inject(LayoutStore);
    expect(layout.isNarrow()).toBe(true);
    expect(dispatch).toHaveBeenCalledWith(appUiActions.sidebarCollapsedSet({ collapsed: true }));
  });
});
