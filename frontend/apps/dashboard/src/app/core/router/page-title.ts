import { inject, Signal } from '@angular/core';
import { marker } from '../i18n/marker';
import { toSignal } from '@angular/core/rxjs-interop';
import { NavigationEnd, Router } from '@angular/router';
import { filter, map } from 'rxjs';
import { formatDate } from '../i18n/format';
import { LocaleService } from '../i18n/locale.service';

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
  | 'platformBilling'
  | 'platformSystemHealth'
  | 'platformAiProviders'
  | 'billing'
  | 'notifications';

/** A page's resolved topbar title/subtitle: catalog keys, and values the subtitle's message needs. */
export interface PageTitleData {
  /** Catalog key of the title. */
  readonly title: string;
  /** Catalog key of the subtitle. */
  readonly subtitle: string;
  readonly subtitleParams?: Readonly<Record<string, unknown>>;
}

/** A `PAGE_TITLES` entry: catalog keys, and values the subtitle needs, computed fresh on each read. */
interface PageTitleEntry {
  readonly title: string;
  readonly subtitle: string;
  /** Computed fresh on each read, for the locale the date is written in. */
  readonly subtitleParams?: (locale: string) => Readonly<Record<string, unknown>>;
}

/** Typed, readonly map of topbar title/subtitle catalog keys for every route that renders inside the Helix shell. */
export const PAGE_TITLES: Readonly<Record<PageTitleKey, PageTitleEntry>> = {
  overview: {
    title: marker('pages.overview.title'),
    subtitle: marker('pages.overview.subtitle'),
    subtitleParams: (locale) => ({ date: formatDate(new Date(), locale, 'longDate') }),
  },
  conversations: {
    title: marker('pages.conversations.title'),
    subtitle: marker('pages.conversations.subtitle'),
  },
  conversationDetail: {
    title: marker('pages.conversationDetail.title'),
    subtitle: marker('pages.conversationDetail.subtitle'),
  },
  customers: {
    title: marker('pages.customers.title'),
    subtitle: marker('pages.customers.subtitle'),
  },
  customerProfile: {
    title: marker('pages.customerProfile.title'),
    subtitle: marker('pages.customerProfile.subtitle'),
  },
  aiAgent: { title: marker('pages.aiAgent.title'), subtitle: marker('pages.aiAgent.subtitle') },
  aiAgentPrompt: {
    title: marker('pages.aiAgentPrompt.title'),
    subtitle: marker('pages.aiAgentPrompt.subtitle'),
  },
  knowledgeBase: {
    title: marker('pages.knowledgeBase.title'),
    subtitle: marker('pages.knowledgeBase.subtitle'),
  },
  knowledgeBaseNew: {
    title: marker('pages.knowledgeBaseNew.title'),
    subtitle: marker('pages.knowledgeBaseNew.subtitle'),
  },
  knowledgeBaseDetail: {
    title: marker('pages.knowledgeBaseDetail.title'),
    subtitle: marker('pages.knowledgeBaseDetail.subtitle'),
  },
  knowledgeBaseEdit: {
    title: marker('pages.knowledgeBaseEdit.title'),
    subtitle: marker('pages.knowledgeBaseEdit.subtitle'),
  },
  integrations: {
    title: marker('pages.integrations.title'),
    subtitle: marker('pages.integrations.subtitle'),
  },
  analytics: {
    title: marker('pages.analytics.title'),
    subtitle: marker('pages.analytics.subtitle'),
  },
  settings: { title: marker('pages.settings.title'), subtitle: marker('pages.settings.subtitle') },
  toolsSettings: {
    title: marker('pages.toolsSettings.title'),
    subtitle: marker('pages.toolsSettings.subtitle'),
  },
  team: { title: marker('pages.team.title'), subtitle: marker('pages.team.subtitle') },
  inviteAccept: {
    title: marker('pages.inviteAccept.title'),
    subtitle: marker('pages.inviteAccept.subtitle'),
  },
  platform: { title: marker('pages.platform.title'), subtitle: marker('pages.platform.subtitle') },
  platformOverview: {
    title: marker('pages.platformOverview.title'),
    subtitle: marker('pages.platformOverview.subtitle'),
  },
  selectTenant: {
    title: marker('pages.selectTenant.title'),
    subtitle: marker('pages.selectTenant.subtitle'),
  },
  platformTenants: {
    title: marker('pages.platformTenants.title'),
    subtitle: marker('pages.platformTenants.subtitle'),
  },
  platformTenantDetail: {
    title: marker('pages.platformTenantDetail.title'),
    subtitle: marker('pages.platformTenantDetail.subtitle'),
  },
  platformTenantNew: {
    title: marker('pages.platformTenantNew.title'),
    subtitle: marker('pages.platformTenantNew.subtitle'),
  },
  escalations: {
    title: marker('pages.escalations.title'),
    subtitle: marker('pages.escalations.subtitle'),
  },
  widgets: { title: marker('pages.widgets.title'), subtitle: marker('pages.widgets.subtitle') },
  billing: { title: marker('pages.billing.title'), subtitle: marker('pages.billing.subtitle') },
  auditLogs: {
    title: marker('pages.auditLogs.title'),
    subtitle: marker('pages.auditLogs.subtitle'),
  },
  platformAuditLogs: {
    title: marker('pages.platformAuditLogs.title'),
    subtitle: marker('pages.platformAuditLogs.subtitle'),
  },
  platformBilling: {
    title: marker('pages.platformBilling.title'),
    subtitle: marker('pages.platformBilling.subtitle'),
  },
  platformSystemHealth: {
    title: marker('pages.platformSystemHealth.title'),
    subtitle: marker('pages.platformSystemHealth.subtitle'),
  },
  platformAiProviders: {
    title: marker('pages.platformAiProviders.title'),
    subtitle: marker('pages.platformAiProviders.subtitle'),
  },
  notifications: {
    title: marker('pages.notifications.title'),
    subtitle: marker('pages.notifications.subtitle'),
  },
} as const;

/** Resolves a `PAGE_TITLES` entry to its keys and the values its subtitle needs now. */
function resolvePageTitle(entry: PageTitleEntry, locale: string): PageTitleData {
  return {
    title: entry.title,
    subtitle: entry.subtitle,
    ...(entry.subtitleParams ? { subtitleParams: entry.subtitleParams(locale) } : {}),
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
  const locale = inject(LocaleService);

  const resolve = (): PageTitleData | undefined => {
    const key = readDeepestPageTitleKey(router);
    return key ? resolvePageTitle(PAGE_TITLES[key], locale.formatLocale()) : undefined;
  };

  return toSignal(
    router.events.pipe(
      filter((event): event is NavigationEnd => event instanceof NavigationEnd),
      map(resolve),
    ),
    { initialValue: resolve() },
  );
}
