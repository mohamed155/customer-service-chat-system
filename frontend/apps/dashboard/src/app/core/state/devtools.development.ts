import { EnvironmentProviders } from '@angular/core';
import { provideStoreDevtools } from '@ngrx/store-devtools';

/** The development build's replacement for `devtools.ts`: Redux DevTools support. */
export const devtoolsProviders: EnvironmentProviders[] = [provideStoreDevtools({ maxAge: 25 })];
