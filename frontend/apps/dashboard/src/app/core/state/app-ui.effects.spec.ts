import { TestBed } from '@angular/core/testing';
import { Actions } from '@ngrx/effects';
import { Subject } from 'rxjs';
import { AppUiEffects } from './app-ui.effects';
import { appUiActions } from './app-ui.feature';

describe('AppUiEffects', () => {
  it('persists theme changes', () => {
    const actions = new Subject<ReturnType<typeof appUiActions.themeModeChanged>>();
    TestBed.configureTestingModule({
      providers: [AppUiEffects, { provide: Actions, useValue: actions }],
    });
    TestBed.inject(AppUiEffects).persistThemeMode.subscribe();
    actions.next(appUiActions.themeModeChanged({ themeMode: 'dark' }));
    expect(localStorage.getItem('app.themeMode')).toBe('dark');
  });
});
