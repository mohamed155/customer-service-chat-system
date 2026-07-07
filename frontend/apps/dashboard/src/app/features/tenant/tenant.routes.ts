import { Routes } from '@angular/router';
import { APP_PATHS } from '../../core/router/app-paths';
import { PAGE_TITLES } from '../../core/router/page-title';

export const TENANT_ROUTES: Routes = [
  { path: '', pathMatch: 'full', redirectTo: APP_PATHS.tenant.overview },
  {
    path: APP_PATHS.tenant.overview,
    loadComponent: () => import('./overview/overview.component').then((m) => m.OverviewComponent),
    data: { pageTitle: 'overview' },
    title: PAGE_TITLES.overview.title,
  },
  {
    path: APP_PATHS.tenant.conversations,
    loadComponent: () =>
      import('./conversations/conversations.component').then((m) => m.ConversationsComponent),
    data: { pageTitle: 'conversations' },
    title: PAGE_TITLES.conversations.title,
  },
  {
    path: APP_PATHS.tenant.customers,
    loadComponent: () =>
      import('./customers/customers.component').then((m) => m.CustomersComponent),
    data: { pageTitle: 'customers' },
    title: PAGE_TITLES.customers.title,
  },
  {
    path: APP_PATHS.tenant.aiAgent,
    loadComponent: () => import('./ai-agent/ai-agent.component').then((m) => m.AiAgentComponent),
    data: { pageTitle: 'aiAgent' },
    title: PAGE_TITLES.aiAgent.title,
  },
  {
    path: APP_PATHS.tenant.knowledgeBase,
    loadComponent: () =>
      import('./knowledge-base/knowledge-base.component').then((m) => m.KnowledgeBaseComponent),
    data: { pageTitle: 'knowledgeBase' },
    title: PAGE_TITLES.knowledgeBase.title,
  },
  {
    path: APP_PATHS.tenant.integrations,
    loadComponent: () =>
      import('./integrations/integrations.component').then((m) => m.IntegrationsComponent),
    data: { pageTitle: 'integrations' },
    title: PAGE_TITLES.integrations.title,
  },
  {
    path: APP_PATHS.tenant.analytics,
    loadComponent: () =>
      import('./analytics/analytics.component').then((m) => m.AnalyticsComponent),
    data: { pageTitle: 'analytics' },
    title: PAGE_TITLES.analytics.title,
  },
  {
    path: APP_PATHS.tenant.settings,
    loadComponent: () => import('./settings/settings.component').then((m) => m.SettingsComponent),
    data: { pageTitle: 'settings' },
    title: PAGE_TITLES.settings.title,
  },
];
