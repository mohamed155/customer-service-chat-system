import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { MockStore, provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { environment } from '../../../environments/environment';
import { APP_CONFIG } from '../../core/config/app-config';
import { ApiErrorNotificationService } from '../../core/errors/api-error-notification.service';
import { appUiActions } from '../../core/state/app-ui.feature';
import { AppShellComponent } from './app-shell.component';
import { LayoutStore } from './layout.store';

describe('AppShellComponent', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      imports: [AppShellComponent],
      providers: [
        provideRouter([]),
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideMockStore({
          initialState: { appUi: { themeMode: 'system', sidebarCollapsed: true } },
        }),
        { provide: APP_CONFIG, useValue: environment },
      ],
    }),
  );

  it('renders semantic landmarks and collapsed navigation', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AppShellComponent);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;
    expect(element.querySelector('aside')).toBeTruthy();
    expect(element.querySelector('header')).toBeTruthy();
    expect(element.querySelector('main')).toBeTruthy();
    expect(element.querySelector('router-outlet')).toBeTruthy();
    expect(element.querySelector('app-sidebar')?.classList.contains('collapsed')).toBe(true);
    expect(element.querySelector('app-breadcrumb')).toBeTruthy();
  });

  it('dispatches the sidebar toggle from the topbar', async () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 1200 });
    await TestBed.compileComponents();
    const store = TestBed.inject(MockStore);
    const dispatch = vi.spyOn(store, 'dispatch');
    const fixture = TestBed.createComponent(AppShellComponent);
    fixture.detectChanges();
    (
      fixture.nativeElement.querySelector('[aria-label="Toggle sidebar"]') as HTMLButtonElement
    ).click();
    expect(dispatch).toHaveBeenCalledWith(appUiActions.sidebarToggled());
  });

  it('renders and dismisses tenant access errors', async () => {
    await TestBed.compileComponents();
    const notifications = TestBed.inject(ApiErrorNotificationService);
    notifications.show("You don't have access to this tenant.");
    const fixture = TestBed.createComponent(AppShellComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain("You don't have access to this tenant.");
    (
      fixture.nativeElement.querySelector(
        '[aria-label="Dismiss tenant access alert"]',
      ) as HTMLButtonElement
    ).click();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).not.toContain(
      "You don't have access to this tenant.",
    );
  });

  it('shows scrim when drawer is open on mobile', async () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 600 });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AppShellComponent);
    fixture.detectChanges();
    const layoutStore = fixture.debugElement.injector.get(LayoutStore);
    layoutStore.openDrawer();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('.scrim')).toBeTruthy();
  });

  it('closes drawer on scrim click', async () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 600 });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AppShellComponent);
    fixture.detectChanges();
    const layoutStore = fixture.debugElement.injector.get(LayoutStore);
    layoutStore.openDrawer();
    fixture.detectChanges();

    (fixture.nativeElement.querySelector('.scrim') as HTMLElement).click();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('.scrim')).toBeNull();
  });

  it('forces sidebar expanded in drawer mode even when store says collapsed', async () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 600 });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AppShellComponent);
    fixture.detectChanges();
    const sidebar: HTMLElement = fixture.nativeElement.querySelector('app-sidebar')!;
    expect(sidebar.classList.contains('collapsed')).toBe(false);
  });

  it('closes drawer on Escape keydown', async () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 600 });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AppShellComponent);
    fixture.detectChanges();
    const layoutStore = fixture.debugElement.injector.get(LayoutStore);
    layoutStore.openDrawer();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('.scrim')).toBeTruthy();
    (fixture.nativeElement.querySelector('.shell') as HTMLElement).dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    );
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('.scrim')).toBeNull();
  });
});
