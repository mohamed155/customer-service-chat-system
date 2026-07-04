import { inject, Injectable } from '@angular/core';
import { Actions, createEffect, ofType } from '@ngrx/effects';
import { tap } from 'rxjs';
import { appUiActions } from './app-ui.feature';

@Injectable()
export class AppUiEffects {
  private readonly actions = inject(Actions);
  readonly persistThemeMode = createEffect(
    () =>
      this.actions.pipe(
        ofType(appUiActions.themeModeChanged),
        tap(({ themeMode }) => {
          if (typeof localStorage !== 'undefined' && typeof localStorage.setItem === 'function')
            localStorage.setItem('app.themeMode', themeMode);
        }),
      ),
    { dispatch: false },
  );
}
