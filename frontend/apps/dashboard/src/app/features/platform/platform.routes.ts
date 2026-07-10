import { Routes } from '@angular/router';
import { APP_PATHS } from '../../core/router/app-paths';

export const PLATFORM_ROUTES: Routes = [
  { path: '', pathMatch: 'full', redirectTo: APP_PATHS.platform.overviewPlaceholder },
  {
    path: APP_PATHS.platform.overviewPlaceholder,
    data: { pageTitle: 'platformOverview' },
    loadComponent: () =>
      import('./overview-placeholder/platform-overview-placeholder.component').then(
        (module) => module.PlatformOverviewPlaceholderComponent,
      ),
  },
];
