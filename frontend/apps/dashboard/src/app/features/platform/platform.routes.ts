import { Routes } from '@angular/router';
import { permissionGuard } from '../../core/authz/permission.guard';
import { PAGE_PERMISSIONS } from '../../core/authz/permissions';
import { APP_PATHS } from '../../core/router/app-paths';

export const PLATFORM_ROUTES: Routes = [
  { path: '', pathMatch: 'full', redirectTo: APP_PATHS.platform.tenants },
  {
    path: APP_PATHS.platform.overviewPlaceholder,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'platformOverview',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.platform.overviewPlaceholder],
    },
    loadComponent: () =>
      import('./overview/platform-overview.component').then((m) => m.PlatformOverviewComponent),
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
  {
    path: APP_PATHS.platform.auditLogs,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'platformAuditLogs',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.platform.auditLogs],
    },
    loadComponent: () =>
      import('./audit-logs/platform-audit-logs.component').then(
        (m) => m.PlatformAuditLogsComponent,
      ),
  },
  {
    path: APP_PATHS.platform.billing,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'platformBilling',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.platform.billing],
    },
    loadComponent: () =>
      import('./billing/platform-billing.component').then((m) => m.PlatformBillingComponent),
  },
  {
    path: APP_PATHS.platform.systemHealth,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'platformSystemHealth',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.platform.systemHealth],
    },
    loadComponent: () =>
      import('./system-health/system-health.component').then((m) => m.SystemHealthComponent),
  },
  {
    path: APP_PATHS.platform.aiProviders,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'platformAiProviders',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.platform.aiProviders],
    },
    loadComponent: () =>
      import('./ai-providers/ai-providers.component').then((m) => m.AiProvidersComponent),
  },
];
