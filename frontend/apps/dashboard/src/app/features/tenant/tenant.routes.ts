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
    data: {
      pageTitle: 'conversations',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.conversations],
    },
    title: PAGE_TITLES.conversations.title,
    children: [
      {
        path: '',
        loadComponent: () =>
          import('./conversations/conversations.component').then((m) => m.ConversationsComponent),
      },
      {
        path: ':id',
        canMatch: [permissionGuard],
        loadComponent: () =>
          import('./conversations/conversation-detail.component').then(
            (m) => m.ConversationDetailComponent,
          ),
        data: {
          pageTitle: 'conversationDetail',
          requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.conversationDetail(':id')],
        },
        title: PAGE_TITLES.conversationDetail.title,
      },
    ],
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
    path: APP_PATHS.tenant.customerDetail(':id'),
    canMatch: [permissionGuard],
    loadComponent: () =>
      import('./customers/customer-profile.component').then((m) => m.CustomerProfileComponent),
    data: {
      pageTitle: 'customerProfile',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.customerDetail(':id')],
    },
    title: PAGE_TITLES.customerProfile.title,
  },
  {
    path: APP_PATHS.tenant.aiAgent,
    canMatch: [permissionGuard],
    data: { pageTitle: 'aiAgent', requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.aiAgent] },
    title: PAGE_TITLES.aiAgent.title,
    children: [
      {
        path: '',
        loadComponent: () =>
          import('./ai-agent/ai-agent.component').then((m) => m.AiAgentComponent),
      },
      {
        path: 'prompt',
        canMatch: [permissionGuard],
        loadComponent: () =>
          import('./ai-agent/prompt/prompt-page.component').then((m) => m.PromptPageComponent),
        data: {
          pageTitle: 'aiAgentPrompt',
          requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.aiAgentPrompt],
        },
        title: PAGE_TITLES.aiAgentPrompt.title,
      },
    ],
  },
  {
    path: APP_PATHS.tenant.knowledgeBase,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'knowledgeBase',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.knowledgeBase],
    },
    title: PAGE_TITLES.knowledgeBase.title,
    children: [
      {
        path: '',
        loadComponent: () =>
          import('./knowledge-base/knowledge-base.component').then((m) => m.KnowledgeBaseComponent),
      },
      {
        path: 'new',
        canMatch: [permissionGuard],
        loadComponent: () =>
          import('./knowledge-base/article-editor.component').then((m) => m.ArticleEditorComponent),
        data: {
          pageTitle: 'knowledgeBaseNew',
          requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.knowledgeBase],
        },
        title: PAGE_TITLES.knowledgeBaseNew.title,
      },
      {
        path: ':id',
        canMatch: [permissionGuard],
        loadComponent: () =>
          import('./knowledge-base/article-detail.component').then((m) => m.ArticleDetailComponent),
        data: {
          pageTitle: 'knowledgeBaseDetail',
          requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.knowledgeBase],
        },
        title: PAGE_TITLES.knowledgeBaseDetail.title,
      },
      {
        path: ':id/edit',
        canMatch: [permissionGuard],
        loadComponent: () =>
          import('./knowledge-base/article-editor.component').then((m) => m.ArticleEditorComponent),
        data: {
          pageTitle: 'knowledgeBaseEdit',
          requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.knowledgeBase],
        },
        title: PAGE_TITLES.knowledgeBaseEdit.title,
      },
    ],
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
    path: APP_PATHS.tenant.auditLogs,
    canMatch: [permissionGuard],
    loadComponent: () =>
      import('./audit-logs/audit-logs.component').then((m) => m.AuditLogsComponent),
    data: {
      pageTitle: 'auditLogs',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.auditLogs],
    },
    title: PAGE_TITLES.auditLogs.title,
  },
  {
    path: APP_PATHS.tenant.settings,
    canMatch: [permissionGuard],
    data: {
      pageTitle: 'settings',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.settings],
    },
    title: PAGE_TITLES.settings.title,
    children: [
      {
        path: '',
        loadComponent: () =>
          import('./settings/settings.component').then((m) => m.SettingsComponent),
      },
      {
        path: 'tools',
        canMatch: [permissionGuard],
        loadComponent: () =>
          import('./settings/tools/tools-settings.component').then((m) => m.ToolsSettingsComponent),
        data: {
          pageTitle: 'toolsSettings',
          requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.settingsTools],
        },
        title: PAGE_TITLES.toolsSettings.title,
      },
    ],
  },
  {
    path: APP_PATHS.tenant.escalations,
    canMatch: [permissionGuard],
    loadComponent: () =>
      import('./escalations/escalation-queue.component').then((m) => m.EscalationQueueComponent),
    data: {
      pageTitle: 'escalations',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.escalations],
    },
    title: PAGE_TITLES.escalations.title,
  },
  {
    path: APP_PATHS.tenant.widgets,
    canMatch: [permissionGuard],
    loadComponent: () => import('./widgets/widgets.component').then((m) => m.WidgetsComponent),
    data: {
      pageTitle: 'widgets',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.widgets],
    },
    title: PAGE_TITLES.widgets.title,
  },
  {
    path: APP_PATHS.tenant.team,
    canMatch: [permissionGuard],
    loadComponent: () => import('./team/team-list.component').then((m) => m.TeamListComponent),
    data: {
      pageTitle: 'team',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.team],
    },
    title: PAGE_TITLES.team.title,
  },
];
