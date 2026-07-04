import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { MockStore, provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { AppComponent } from './app.component';
import { selectThemeMode } from './core/state/app-ui.feature';

describe('application theme', () => {
  it('applies explicit and live system theme changes', async () => {
    const target = new EventTarget();
    const media = Object.assign(target, {
      matches: false,
      media: '(prefers-color-scheme: dark)',
      onchange: null,
      addListener: () => undefined,
      removeListener: () => undefined,
    }) as MediaQueryList;
    vi.stubGlobal('matchMedia', vi.fn().mockReturnValue(media));
    await TestBed.configureTestingModule({
      imports: [AppComponent],
      providers: [
        provideRouter([]),
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideMockStore({
          initialState: { appUi: { themeMode: 'light', sidebarCollapsed: false } },
        }),
      ],
    }).compileComponents();
    const store = TestBed.inject(MockStore);
    const fixture = TestBed.createComponent(AppComponent);
    fixture.detectChanges();
    expect(document.documentElement.dataset['theme']).toBe('light');
    store.overrideSelector(selectThemeMode, 'system');
    store.refreshState();
    const change = new Event('change') as MediaQueryListEvent;
    Object.defineProperty(change, 'matches', { value: true });
    media.dispatchEvent(change);
    fixture.detectChanges();
    expect(document.documentElement.dataset['theme']).toBe('dark');
    expect(fixture.nativeElement.querySelector('tui-root').getAttribute('tuiTheme')).toBe('dark');
  });
});
