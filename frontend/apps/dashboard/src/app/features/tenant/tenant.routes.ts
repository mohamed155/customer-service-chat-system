import { Routes } from '@angular/router';
import { APP_PATHS } from '../../core/router/app-paths';

export const TENANT_ROUTES: Routes = [
  { path: '', pathMatch: 'full', redirectTo: APP_PATHS.tenant.overviewPlaceholder },
  {
    path: APP_PATHS.tenant.overviewPlaceholder,
    loadComponent: () =>
      import('./overview-placeholder/tenant-overview-placeholder.component').then(
        (module) => module.TenantOverviewPlaceholderComponent,
      ),
  },
];
