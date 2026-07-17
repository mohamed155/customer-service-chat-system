import { Injectable, InjectionToken } from '@angular/core';
import { Observable, of } from 'rxjs';
import {
  ANALYTICS_CHARTS,
  ANALYTICS_METRICS,
  CHANNEL_BREAKDOWN,
  OVERVIEW_METRICS,
  OVERVIEW_TREND_SERIES,
  TOP_ARTICLES,
} from '../../shared/fixtures/analytics.fixtures';
import { CONVERSATION_FIXTURES } from '../../shared/fixtures/conversation.fixtures';
import { CUSTOMER_FIXTURES } from '../../shared/fixtures/customer.fixtures';
import {
  AlertFixture,
  ApiKeyFixture,
  ChannelBreakdownFixture,
  ConversationFixture,
  CustomerFixture,
  IntegrationFixture,
  InvoiceFixture,
  MetricFixture,
  SessionFixture,
  TeamMemberFixture,
  TopArticleFixture,
  TrendSeriesFixture,
  UsageFixture,
  WorkspaceProfileFixture,
} from '../../shared/fixtures/fixture.models';
import { INTEGRATION_FIXTURES } from '../../shared/fixtures/integration.fixtures';
import {
  API_KEY_FIXTURE,
  INVOICE_FIXTURES,
  OVERVIEW_ALERT,
  SESSION_FIXTURES,
  TEAM_MEMBERS,
  USAGE_FIXTURES,
  WORKSPACE_PROFILE,
} from '../../shared/fixtures/settings.fixtures';

export const PAGE_ROUTE = new InjectionToken<string>(
  'Page route identifier for RoutedPageDataService',
);

export type OverviewPayload = {
  alert: AlertFixture;
  metrics: readonly MetricFixture[];
  trendSeries: readonly TrendSeriesFixture[];
  breakdown: readonly ChannelBreakdownFixture[];
  recentConversations: readonly ConversationFixture[];
  customers: readonly CustomerFixture[];
};

export type AiAgentPayload = {
  allowedTopics: string[];
  blockedTopics: string[];
  escalationRules: string[];
  timelineSteps: { label: string; detail: string }[];
};

export type AnalyticsPayload = {
  metrics: readonly MetricFixture[];
  charts: readonly TrendSeriesFixture[];
  topArticles: readonly TopArticleFixture[];
};

export type SettingsPayload = {
  profile: WorkspaceProfileFixture;
  team: readonly TeamMemberFixture[];
  usage: readonly UsageFixture[];
  invoices: readonly InvoiceFixture[];
  apiKey: ApiKeyFixture;
  sessions: readonly SessionFixture[];
};

export type ConversationsPayload = {
  conversations: readonly ConversationFixture[];
  customers: readonly CustomerFixture[];
};

export type PagePayload =
  | { page: 'overview'; data: OverviewPayload }
  | { page: 'customers'; data: readonly CustomerFixture[] }
  | { page: 'conversations'; data: ConversationsPayload }
  | { page: 'ai-agent'; data: AiAgentPayload }
  | { page: 'integrations'; data: readonly IntegrationFixture[] }
  | { page: 'analytics'; data: AnalyticsPayload }
  | { page: 'settings'; data: SettingsPayload };

@Injectable({ providedIn: 'root' })
export class RoutedPageDataService {
  load(page: string, tenantId: string | null): Observable<PagePayload> {
    const payload = this.buildPayload(page, tenantId);
    return of(payload);
  }

  private buildPayload(page: string, tenantId: string | null): PagePayload {
    if (tenantId === 'empty') {
      return this.emptyPayload(page);
    }
    if (tenantId === 'tenant-b') {
      return this.tenantBPayload(page);
    }
    return this.tenantAPayload(page);
  }

  private tenantAPayload(page: string): PagePayload {
    switch (page) {
      case 'overview':
        return {
          page: 'overview',
          data: {
            alert: OVERVIEW_ALERT,
            metrics: OVERVIEW_METRICS,
            trendSeries: OVERVIEW_TREND_SERIES,
            breakdown: CHANNEL_BREAKDOWN,
            recentConversations: CONVERSATION_FIXTURES.slice(0, 5),
            customers: CUSTOMER_FIXTURES,
          },
        };
      case 'customers':
        return { page: 'customers', data: CUSTOMER_FIXTURES };
      case 'conversations':
        return {
          page: 'conversations',
          data: { conversations: CONVERSATION_FIXTURES, customers: CUSTOMER_FIXTURES },
        };
      case 'ai-agent':
        return {
          page: 'ai-agent',
          data: {
            allowedTopics: ['Shipping', 'Returns', 'Billing', 'Warranty'],
            blockedTopics: ['Legal advice', 'Medical claims', 'Payment secrets'],
            escalationRules: [
              'Customer sentiment is angry and confidence drops below 75%',
              'Repeated answer loop detected within two AI turns',
              'Warranty or billing policy conflict requires human review',
            ],
            timelineSteps: [
              { label: 'Classify intent', detail: 'Exchange request with promotional credit' },
              { label: 'Retrieve knowledge', detail: 'Returns and exchanges policy' },
              { label: 'Draft answer', detail: 'Preserve eligible credit and explain next step' },
            ],
          },
        };
      case 'integrations':
        return { page: 'integrations', data: INTEGRATION_FIXTURES };
      case 'analytics':
        return {
          page: 'analytics',
          data: {
            metrics: ANALYTICS_METRICS.slice(0, 6),
            charts: ANALYTICS_CHARTS,
            topArticles: TOP_ARTICLES,
          },
        };
      case 'settings':
        return {
          page: 'settings',
          data: {
            profile: WORKSPACE_PROFILE,
            team: TEAM_MEMBERS,
            usage: USAGE_FIXTURES,
            invoices: INVOICE_FIXTURES,
            apiKey: API_KEY_FIXTURE,
            sessions: SESSION_FIXTURES,
          },
        };
      default:
        throw new Error(`Unknown page: ${page}`);
    }
  }

  private tenantBPayload(page: string): PagePayload {
    switch (page) {
      case 'overview':
        return {
          page: 'overview',
          data: {
            alert: OVERVIEW_ALERT,
            metrics: OVERVIEW_METRICS.slice(0, 3),
            trendSeries: OVERVIEW_TREND_SERIES.slice(0, 1),
            breakdown: CHANNEL_BREAKDOWN.slice(0, 2),
            recentConversations: CONVERSATION_FIXTURES.slice(0, 2),
            customers: CUSTOMER_FIXTURES.slice(2, 4),
          },
        };
      case 'customers':
        return { page: 'customers', data: CUSTOMER_FIXTURES.slice(2, 4) };
      case 'conversations':
        return {
          page: 'conversations',
          data: {
            conversations: CONVERSATION_FIXTURES.slice(0, 3),
            customers: CUSTOMER_FIXTURES.slice(2, 4),
          },
        };
      case 'ai-agent':
        return {
          page: 'ai-agent',
          data: {
            allowedTopics: ['Shipping', 'Returns'],
            blockedTopics: ['Legal advice'],
            escalationRules: ['Customer sentiment is angry and confidence drops below 75%'],
            timelineSteps: [{ label: 'Classify intent', detail: 'Support request' }],
          },
        };
      case 'integrations':
        return { page: 'integrations', data: INTEGRATION_FIXTURES.slice(0, 1) };
      case 'analytics':
        return {
          page: 'analytics',
          data: {
            metrics: ANALYTICS_METRICS.slice(0, 3),
            charts: ANALYTICS_CHARTS.slice(0, 1),
            topArticles: TOP_ARTICLES.slice(0, 2),
          },
        };
      case 'settings':
        return {
          page: 'settings',
          data: {
            profile: WORKSPACE_PROFILE,
            team: TEAM_MEMBERS.slice(0, 2),
            usage: USAGE_FIXTURES.slice(0, 1),
            invoices: INVOICE_FIXTURES.slice(0, 1),
            apiKey: API_KEY_FIXTURE,
            sessions: SESSION_FIXTURES.slice(0, 1),
          },
        };
      default:
        return this.emptyPayload(page);
    }
  }

  private emptyPayload(page: string): PagePayload {
    switch (page) {
      case 'overview':
        return {
          page: 'overview',
          data: {
            alert: OVERVIEW_ALERT,
            metrics: [],
            trendSeries: [],
            breakdown: [],
            recentConversations: [],
            customers: [],
          },
        };
      case 'customers':
        return { page: 'customers', data: [] };
      case 'conversations':
        return { page: 'conversations', data: { conversations: [], customers: [] } };
      case 'ai-agent':
        return {
          page: 'ai-agent',
          data: {
            allowedTopics: [],
            blockedTopics: [],
            escalationRules: [],
            timelineSteps: [],
          },
        };
      case 'integrations':
        return { page: 'integrations', data: [] };
      case 'analytics':
        return { page: 'analytics', data: { metrics: [], charts: [], topArticles: [] } };
      case 'settings':
        return {
          page: 'settings',
          data: {
            profile: WORKSPACE_PROFILE,
            team: [],
            usage: [],
            invoices: [],
            apiKey: API_KEY_FIXTURE,
            sessions: [],
          },
        };
      default:
        throw new Error(`Unknown page: ${page}`);
    }
  }
}
