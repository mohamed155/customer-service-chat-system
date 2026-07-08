import { Routes } from '@angular/router';
import { areaAccessGuard } from './core/router/area-access.guard';
import { APP_PATHS } from './core/router/app-paths';
import { PAGE_TITLES } from './core/router/page-title';
import { AppShellComponent } from './layout/app-shell/app-shell.component';

export const routes: Routes = [
  {
    path: APP_PATHS.root,
    pathMatch: 'full',
    redirectTo: `${APP_PATHS.tenant.base}/${APP_PATHS.tenant.overview}`,
  },
  {
    path: APP_PATHS.auth.base,
    loadChildren: () => import('./features/auth/auth.routes').then((module) => module.AUTH_ROUTES),
  },
  {
    path: '',
    component: AppShellComponent,
    children: [
      {
        path: APP_PATHS.platform.base,
        canMatch: [areaAccessGuard],
        data: { area: 'platform', pageTitle: 'platform' },
        title: PAGE_TITLES.platform.title,
        loadChildren: () =>
          import('./features/platform/platform.routes').then((module) => module.PLATFORM_ROUTES),
      },
      {
        path: APP_PATHS.tenant.base,
        canMatch: [areaAccessGuard],
        data: { area: 'tenant' },
        loadChildren: () =>
          import('./features/tenant/tenant.routes').then((module) => module.TENANT_ROUTES),
      },
    ],
  },
  {
    path: APP_PATHS.notFound,
    loadComponent: () =>
      import('./features/not-found/not-found.component').then((module) => module.NotFoundComponent),
  },
];
