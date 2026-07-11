import { TestBed } from '@angular/core/testing';
import { provideMockStore, MockStore } from '@ngrx/store/testing';
import { provideRouter, Router } from '@angular/router';
import { Component } from '@angular/core';
import { appUiActions } from '../../core/state/app-ui.feature';
import { LayoutStore } from './layout.store';

@Component({ template: '', standalone: true })
class TestCmp {}

describe('LayoutStore', () => {
  it('collapses globally when initialized in a narrow viewport', () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 800 });
    TestBed.configureTestingModule({
      providers: [LayoutStore, provideMockStore(), provideRouter([])],
    });
    const globalStore = TestBed.inject(MockStore);
    const dispatch = vi.spyOn(globalStore, 'dispatch');
    const layout = TestBed.inject(LayoutStore);
    expect(layout.isNarrow()).toBe(true);
    expect(dispatch).toHaveBeenCalledWith(appUiActions.sidebarCollapsedSet({ collapsed: true }));
  });

  it('returns isMobile true when viewport is below mobile breakpoint', () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 500 });
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [LayoutStore, provideMockStore(), provideRouter([])],
    });
    const layout = TestBed.inject(LayoutStore);
    expect(layout.isMobile()).toBe(true);
  });

  it('returns isMobile false when viewport is above mobile breakpoint', () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 1024 });
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [LayoutStore, provideMockStore(), provideRouter([])],
    });
    const layout = TestBed.inject(LayoutStore);
    expect(layout.isMobile()).toBe(false);
  });

  it('toggles drawerOpen with openDrawer and closeDrawer', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [LayoutStore, provideMockStore(), provideRouter([])],
    });
    const layout = TestBed.inject(LayoutStore);
    expect(layout.drawerOpen()).toBe(false);
    layout.openDrawer();
    expect(layout.drawerOpen()).toBe(true);
    layout.closeDrawer();
    expect(layout.drawerOpen()).toBe(false);
  });

  it('closes drawer when viewport resizes from mobile to desktop', () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 500 });
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [LayoutStore, provideMockStore(), provideRouter([])],
    });
    const layout = TestBed.inject(LayoutStore);
    layout.openDrawer();
    expect(layout.drawerOpen()).toBe(true);
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 1024 });
    window.dispatchEvent(new Event('resize'));
    expect(layout.drawerOpen()).toBe(false);
  });

  it('closes drawer on navigation', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [
        LayoutStore,
        provideMockStore(),
        provideRouter([{ path: 'test', component: TestCmp }]),
      ],
    });
    const layout = TestBed.inject(LayoutStore);
    const router = TestBed.inject(Router);
    layout.openDrawer();
    expect(layout.drawerOpen()).toBe(true);
    await router.navigateByUrl('/test');
    expect(layout.drawerOpen()).toBe(false);
  });
});
