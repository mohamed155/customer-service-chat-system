import { Routes } from '@angular/router';
import { permissionGuard } from '../../core/authz/permission.guard';
import { PAGE_PERMISSIONS } from '../../core/authz/permissions';
import { APP_PATHS } from '../../core/router/app-paths';
import { PAGE_TITLES } from '../../core/router/page-title';

export const TENANT_ROUTES: Routes = [
  { path: '', pathMatch: 'full', redirectTo: APP_PATHS.tenant.overview },
  {
    path: APP_PATHS.tenant.overview,
    canMatch: [permissionGuard],
    loadComponent: () => import('./overview/overview.component').then((m) => m.OverviewComponent),
    data: {
      pageTitle: 'overview',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.overview],
    },
    title: PAGE_TITLES.overview.title,
  },
  {
    path: APP_PATHS.tenant.conversations,
    canMatch: [permissionGuard],
    loadComponent: () =>
      import('./conversations/conversations.component').then((m) => m.ConversationsComponent),
    data: {
      pageTitle: 'conversations',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.conversations],
    },
    title: PAGE_TITLES.conversations.title,
  },
  {
    path: APP_PATHS.tenant.customers,
    canMatch: [permissionGuard],
    loadComponent: () =>
      import('./customers/customers.component').then((m) => m.CustomersComponent),
    data: {
      pageTitle: 'customers',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.customers],
    },
    title: PAGE_TITLES.customers.title,
  },
  {
    path: APP_PATHS.tenant.aiAgent,
    canMatch: [permissionGuard],
    loadComponent: () => import('./ai-agent/ai-agent.component').then((m) => m.AiAgentComponent),
    data: { pageTitle: 'aiAgent', requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.aiAgent] },
    title: PAGE_TITLES.aiAgent.title,
  },
  {
    path: APP_PATHS.tenant.knowledgeBase,
    canMatch: [permissionGuard],
    loadComponent: () =>
      import('./knowledge-base/knowledge-base.component').then((m) => m.KnowledgeBaseComponent),
    data: {
      pageTitle: 'knowledgeBase',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.knowledgeBase],
    },
    title: PAGE_TITLES.knowledgeBase.title,
  },
  {
    path: APP_PATHS.tenant.integrations,
    canMatch: [permissionGuard],
    loadComponent: () =>
      import('./integrations/integrations.component').then((m) => m.IntegrationsComponent),
    data: {
      pageTitle: 'integrations',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.integrations],
    },
    title: PAGE_TITLES.integrations.title,
  },
  {
    path: APP_PATHS.tenant.analytics,
    canMatch: [permissionGuard],
    loadComponent: () =>
      import('./analytics/analytics.component').then((m) => m.AnalyticsComponent),
    data: {
      pageTitle: 'analytics',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.analytics],
    },
    title: PAGE_TITLES.analytics.title,
  },
  {
    path: APP_PATHS.tenant.settings,
    canMatch: [permissionGuard],
    loadComponent: () => import('./settings/settings.component').then((m) => m.SettingsComponent),
    data: {
      pageTitle: 'settings',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.settings],
    },
    title: PAGE_TITLES.settings.title,
  },
];
