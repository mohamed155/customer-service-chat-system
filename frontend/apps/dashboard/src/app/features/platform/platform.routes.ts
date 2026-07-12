import { Routes } from '@angular/router';
import { permissionGuard } from '../../core/authz/permission.guard';
import { APP_PATHS } from '../../core/router/app-paths';

export const PLATFORM_ROUTES: Routes = [
  { path: '', pathMatch: 'full', redirectTo: APP_PATHS.platform.tenants },
  {
    path: APP_PATHS.platform.overviewPlaceholder,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'platformOverview',
      requiredPermission: 'platform.admin',
    },
    loadComponent: () =>
      import('./overview-placeholder/platform-overview-placeholder.component').then(
        (module) => module.PlatformOverviewPlaceholderComponent,
      ),
  },
  {
    path: APP_PATHS.platform.tenants,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'platformTenants',
      requiredPermission: 'platform.tenants.list',
    },
    loadComponent: () =>
      import('./tenants/tenant-list.component').then((module) => module.TenantListComponent),
  },
  {
    path: `${APP_PATHS.platform.tenants}/${APP_PATHS.platform.newTenant}`,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'platformTenantNew',
      requiredPermission: 'platform.tenants.list',
    },
    loadComponent: () =>
      import('./tenants/tenant-form.component').then((module) => module.TenantFormComponent),
  },
  {
    path: `${APP_PATHS.platform.tenants}/:id/edit`,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'platformTenantDetail',
      requiredPermission: 'platform.tenants.list',
    },
    loadComponent: () =>
      import('./tenants/tenant-form.component').then((module) => module.TenantFormComponent),
  },
  {
    path: `${APP_PATHS.platform.tenants}/:id`,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'platformTenantDetail',
      requiredPermission: 'platform.tenants.list',
    },
    loadComponent: () =>
      import('./tenants/tenant-detail.component').then((module) => module.TenantDetailComponent),
  },
];
