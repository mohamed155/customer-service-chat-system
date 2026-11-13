import { Routes } from '@angular/router';
import { permissionGuard } from './core/authz/permission.guard';
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
    path: `${APP_PATHS.invite}/:token`,
    loadComponent: () =>
      import('./features/auth/invite/accept-invitation.component').then(
        (m) => m.AcceptInvitationComponent,
      ),
    data: { pageTitle: 'inviteAccept' },
  },
  {
    // Reached by a signed-in user whose account needs a second factor; outside the shell and the
    // guest-only auth routes.
    path: APP_PATHS.mfaSetup,
    canMatch: [authGuard],
    loadComponent: () =>
      import('./features/auth/mfa-setup/mfa-setup.component').then((m) => m.MfaSetupComponent),
    title: 'Set up two-step verification',
  },
  {
    path: '',
    component: AppShellComponent,
    canMatch: [authGuard],
    children: [
      {
        path: APP_PATHS.security,
        loadComponent: () =>
          import('./features/account/security.component').then((m) => m.SecurityComponent),
        title: 'Security',
      },
      {
        path: APP_PATHS.platform.base,
        canMatch: [areaAccessGuard, permissionGuard],
        data: {
          area: 'platform',
          requiredPermission: 'platform.tenants.list',
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
        // `preload`: the area's routes are looked at for preloading only once the area is loaded.
        data: { area: 'tenant', preload: true },
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
