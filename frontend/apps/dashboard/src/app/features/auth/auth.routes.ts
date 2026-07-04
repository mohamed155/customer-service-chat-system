import { Routes } from '@angular/router';
import { APP_PATHS } from '../../core/router/app-paths';

export const AUTH_ROUTES: Routes = [
  { path: '', pathMatch: 'full', redirectTo: APP_PATHS.auth.loginPlaceholder },
  {
    path: APP_PATHS.auth.loginPlaceholder,
    loadComponent: () =>
      import('./login-placeholder/login-placeholder.component').then(
        (module) => module.LoginPlaceholderComponent,
      ),
  },
];
