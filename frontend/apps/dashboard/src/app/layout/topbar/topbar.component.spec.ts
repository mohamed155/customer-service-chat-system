import { Component } from '@angular/core';
import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Router, RouterOutlet, provideRouter } from '@angular/router';
import { MockStore, provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { appUiActions } from '../../core/state/app-ui.feature';
import { TopbarComponent } from './topbar.component';

@Component({
  imports: [RouterOutlet, TopbarComponent],
  template: `<app-topbar /><router-outlet />`,
})
class HostComponent {}

@Component({ template: `` })
class EmptyComponent {}

describe('TopbarComponent', () => {
  async function setup(themeMode: 'light' | 'dark' | 'system' = 'light') {
    TestBed.configureTestingModule({
      imports: [HostComponent],
      providers: [
        provideRouter([
          {
            path: 'tenant/conversations',
            component: EmptyComponent,
            data: { pageTitle: 'conversations' },
          },
        ]),
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideMockStore({
          initialState: { appUi: { themeMode, sidebarCollapsed: false } },
        }),
      ],
    });
    await TestBed.compileComponents();
    const router = TestBed.inject(Router);
    await router.navigateByUrl('/tenant/conversations');
    const fixture = TestBed.createComponent(HostComponent);
    fixture.detectChanges();
    return { fixture, store: TestBed.inject(MockStore) };
  }

  it('renders title and subtitle from route data', async () => {
    const { fixture } = await setup();
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';

    expect(text).toContain('Conversations');
    expect(text).toContain('Shared inbox · 6 open, 2 escalated');
  });

  it('dispatches the next theme mode when the theme button is clicked', async () => {
    const { fixture, store } = await setup('dark');
    const dispatch = vi.spyOn(store, 'dispatch');
    const themeButton = (fixture.nativeElement as HTMLElement).querySelector(
      'button[aria-label^="Theme is dark"]',
    ) as HTMLButtonElement;

    themeButton.click();

    expect(dispatch).toHaveBeenCalledWith(appUiActions.themeModeChanged({ themeMode: 'system' }));
  });

  it('keeps search, notifications, and New as visual controls', async () => {
    const { fixture, store } = await setup();
    const dispatch = vi.spyOn(store, 'dispatch');
    const element = fixture.nativeElement as HTMLElement;

    (element.querySelector('input[type="search"]') as HTMLInputElement).dispatchEvent(
      new Event('input'),
    );
    (element.querySelector('[aria-label="Notifications"]') as HTMLElement).click();
    (element.querySelector('.new-button') as HTMLButtonElement).click();

    expect(dispatch).not.toHaveBeenCalled();
  });
});
