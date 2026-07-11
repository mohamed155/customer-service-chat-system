import { Routes } from '@angular/router';
import { permissionGuard } from './core/authz/permission.guard';
import { PAGE_PERMISSIONS } from './core/authz/permissions';
import { areaAccessGuard } from './core/router/area-access.guard';
import { authGuard } from './core/router/auth.guard';
import { guestGuard } from './core/router/guest.guard';
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
    canMatch: [guestGuard],
    loadChildren: () => import('./features/auth/auth.routes').then((module) => module.AUTH_ROUTES),
  },
  {
    path: '',
    component: AppShellComponent,
    canMatch: [authGuard],
    children: [
      {
        path: APP_PATHS.platform.base,
        canMatch: [areaAccessGuard, permissionGuard],
        data: {
          area: 'platform',
          requiredPermission: PAGE_PERMISSIONS[APP_PATHS.platform.base],
        },
        title: PAGE_TITLES.platform.title,
        loadChildren: () =>
          import('./features/platform/platform.routes').then((module) => module.PLATFORM_ROUTES),
      },
      {
        path: `${APP_PATHS.tenant.base}/${APP_PATHS.tenant.select}`,
        loadComponent: () =>
          import('./features/tenant/tenant-select/tenant-select.component').then(
            (module) => module.TenantSelectComponent,
          ),
        data: { pageTitle: 'selectTenant' },
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
