import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MockStore, provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { appUiActions } from '../../../core/state/app-ui.feature';
import { SettingsComponent } from './settings.component';

describe('SettingsComponent', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      imports: [SettingsComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideMockStore({
          initialState: { appUi: { themeMode: 'light', sidebarCollapsed: false } },
        }),
      ],
    }),
  );

  it('dispatches theme changes from the General tab', async () => {
    await TestBed.compileComponents();
    const store = TestBed.inject(MockStore);
    const dispatch = vi.spyOn(store, 'dispatch');
    const fixture = TestBed.createComponent(SettingsComponent);
    fixture.detectChanges();

    (
      Array.from(
        (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>(
          '.segmented button',
        ),
      ).find((button) => button.textContent?.trim() === 'dark') as HTMLButtonElement
    ).click();

    expect(dispatch).toHaveBeenCalledWith(appUiActions.themeModeChanged({ themeMode: 'dark' }));
  });
});
