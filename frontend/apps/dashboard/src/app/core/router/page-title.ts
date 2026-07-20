import { inject, Signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { NavigationEnd, Router } from '@angular/router';
import { filter, map } from 'rxjs';

/** Stable key identifying a routed page, used as `route.data['pageTitle']` and as a `PAGE_TITLES` lookup key. */
export type PageTitleKey =
  | 'overview'
  | 'conversations'
  | 'conversationDetail'
  | 'customers'
  | 'customerProfile'
  | 'aiAgent'
  | 'aiAgentPrompt'
  | 'knowledgeBase'
  | 'knowledgeBaseNew'
  | 'knowledgeBaseDetail'
  | 'knowledgeBaseEdit'
  | 'integrations'
  | 'analytics'
  | 'settings'
  | 'toolsSettings'
  | 'team'
  | 'inviteAccept'
  | 'platform'
  | 'platformOverview'
  | 'selectTenant'
  | 'platformTenants'
  | 'platformTenantDetail'
  | 'platformTenantNew'
  | 'escalations'
  | 'widgets'
  | 'auditLogs'
  | 'platformAuditLogs'
  | 'notifications';

/** A page's resolved topbar title/subtitle. */
export interface PageTitleData {
  readonly title: string;
  readonly subtitle: string;
}

/** A `PAGE_TITLES` entry — subtitle is either static text or a function computed fresh on each read. */
interface PageTitleEntry {
  readonly title: string;
  readonly subtitle: string | (() => string);
}

/** Formats today's date as e.g. "Tuesday, June 20 · Your support cockpit" (computed fresh, not baked in). */
function overviewSubtitle(): string {
  const formattedDate = new Date().toLocaleDateString('en-US', {
    weekday: 'long',
    month: 'long',
    day: 'numeric',
  });
  return `${formattedDate} · Your support cockpit`;
}

/** Typed, readonly map of topbar title/subtitle text for every route that renders inside the Helix shell. */
export const PAGE_TITLES: Readonly<Record<PageTitleKey, PageTitleEntry>> = {
  overview: { title: 'Overview', subtitle: overviewSubtitle },
  conversations: { title: 'Conversations', subtitle: 'Manage your team conversations' },
  conversationDetail: {
    title: 'Conversation',
    subtitle: 'Customer conversation details and timeline',
  },
  customers: { title: 'Customers', subtitle: 'Customer profiles and conversation history' },
  customerProfile: {
    title: 'Customer profile',
    subtitle: 'Contact, identifiers, and conversation history',
  },
  aiAgent: { title: 'AI Agent', subtitle: 'Configure how your assistant behaves' },
  aiAgentPrompt: {
    title: 'System Prompt',
    subtitle: 'Manage the base instruction given to the AI',
  },
  knowledgeBase: {
    title: 'Knowledge Base',
    subtitle: 'Train your AI with trusted company knowledge',
  },
  knowledgeBaseNew: {
    title: 'New Article',
    subtitle: 'Create a knowledge article',
  },
  knowledgeBaseDetail: {
    title: 'Article',
    subtitle: 'Knowledge article details',
  },
  knowledgeBaseEdit: {
    title: 'Edit Article',
    subtitle: 'Update the knowledge article',
  },
  integrations: { title: 'Integrations', subtitle: 'Connect channels and business systems' },
  analytics: { title: 'Analytics', subtitle: 'Trends across every channel' },
  settings: { title: 'Settings', subtitle: 'Workspace preferences and security' },
  toolsSettings: {
    title: 'AI Tool Settings',
    subtitle: 'Manage built-in and custom AI tools',
  },
  team: { title: 'Team', subtitle: 'Manage team members and invitations' },
  inviteAccept: { title: 'Accept invitation', subtitle: 'Join your team' },
  platform: { title: 'Platform', subtitle: 'Platform administration' },
  platformOverview: { title: 'Platform overview', subtitle: 'Platform administration' },
  selectTenant: { title: 'Select tenant', subtitle: 'Choose a tenant context to continue' },
  platformTenants: { title: 'Tenants', subtitle: 'Manage customer organizations' },
  platformTenantDetail: { title: 'Tenant details', subtitle: 'View and manage this organization' },
  platformTenantNew: { title: 'New tenant', subtitle: 'Onboard a new customer organization' },
  escalations: { title: 'Escalations', subtitle: 'Manage queued and active escalations' },
  widgets: {
    title: 'Chat Widget',
    subtitle: 'Embed the chat widget on your website',
  },
  auditLogs: { title: 'Audit Logs', subtitle: 'Track sensitive actions in your workspace' },
  platformAuditLogs: {
    title: 'Audit Logs',
    subtitle: 'Platform-wide activity across all tenants',
  },
  notifications: {
    title: 'Notifications',
    subtitle: 'Your recent activity and alerts',
  },
} as const;

/** Resolves a `PAGE_TITLES` entry to its current title/subtitle, evaluating a function subtitle fresh. */
function resolvePageTitle(entry: PageTitleEntry): PageTitleData {
  return {
    title: entry.title,
    subtitle: typeof entry.subtitle === 'function' ? entry.subtitle() : entry.subtitle,
  };
}

/** Reads `route.data['pageTitle']` off the deepest activated route snapshot, if present. */
function readDeepestPageTitleKey(router: Router): PageTitleKey | undefined {
  let route = router.routerState.snapshot.root;
  while (route.firstChild) {
    route = route.firstChild;
  }
  return route.data['pageTitle'] as PageTitleKey | undefined;
}

/**
 * Injection-context utility returning a `Signal` of the current page's resolved title/subtitle,
 * reactive to route changes. Reads `route.data['pageTitle']` from the deepest activated route and
 * resolves it through `PAGE_TITLES`. Returns `undefined` when the active route has no `pageTitle`
 * data (e.g. auth routes, not-found route).
 */
export function injectPageTitle(): Signal<PageTitleData | undefined> {
  const router = inject(Router);

  const resolve = (): PageTitleData | undefined => {
    const key = readDeepestPageTitleKey(router);
    return key ? resolvePageTitle(PAGE_TITLES[key]) : undefined;
  };

  return toSignal(
    router.events.pipe(
      filter((event): event is NavigationEnd => event instanceof NavigationEnd),
      map(resolve),
    ),
    { initialValue: resolve() },
  );
}
