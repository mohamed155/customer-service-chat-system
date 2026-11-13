import { EnvironmentProviders } from '@angular/core';

/**
 * Store devtools. None in a production build: `@ngrx/store-devtools` stays out of the bundle
 * (about 15 kB of the initial download). `devtools.development.ts` replaces this file in the
 * `development` configuration (angular.json, fileReplacements) and adds them (MOH-171).
 */
export const devtoolsProviders: EnvironmentProviders[] = [];
