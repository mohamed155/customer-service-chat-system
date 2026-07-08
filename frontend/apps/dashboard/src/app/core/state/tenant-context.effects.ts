import { inject, Injectable } from '@angular/core';
import { Actions, createEffect, ofType } from '@ngrx/effects';
import { tap } from 'rxjs';
import { tenantContextActions } from './tenant-context.feature';

@Injectable()
export class TenantContextEffects {
  private readonly actions = inject(Actions);

  readonly persistActiveTenant = createEffect(
    () =>
      this.actions.pipe(
        ofType(tenantContextActions.setActiveTenant, tenantContextActions.switchTenantSucceeded),
        tap(({ tenant }) => {
          if (typeof localStorage !== 'undefined' && typeof localStorage.setItem === 'function')
            localStorage.setItem('app.tenant', JSON.stringify(tenant));
        }),
      ),
    { dispatch: false },
  );

  readonly clearPersistedTenant = createEffect(
    () =>
      this.actions.pipe(
        ofType(tenantContextActions.clearActiveTenant),
        tap(() => {
          if (typeof localStorage !== 'undefined' && typeof localStorage.removeItem === 'function')
            localStorage.removeItem('app.tenant');
        }),
      ),
    { dispatch: false },
  );
}
