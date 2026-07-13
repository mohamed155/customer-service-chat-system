export type Channel = 'web' | 'whatsapp' | 'telegram' | 'mobile-sdk' | 'email' | 'phone';
export type ConversationStatus = 'open' | 'escalated' | 'closed';
export type Sentiment = 'positive' | 'neutral' | 'angry';
export type MessageAuthor = 'customer' | 'ai' | 'human' | 'system';
export type ArticleStatus = 'draft' | 'published' | 'archived';
export type ArticleSource = 'article' | 'faq' | 'pdf' | 'url';
export type IntegrationStatus = 'connected' | 'not-connected' | 'coming-soon';
export type DeltaDirection = 'up' | 'down';

export interface ConversationFixture {
  id: string;
  customerId: string;
  channel: Exclude<Channel, 'mobile-sdk'>;
  status: ConversationStatus;
  sentiment: Sentiment;
  snippet: string;
  updatedAt: string;
  unread: boolean;
  messages: readonly MessageFixture[];
}

export interface MessageFixture {
  id: string;
  author: MessageAuthor;
  body: string;
  createdAt: string;
  aiConfidence?: number;
  citations?: readonly string[];
}

export interface CustomerFixture {
  id: string;
  name: string;
  email: string;
  avatarInitials: string;
  channel: Channel;
  tier: 'free' | 'pro' | 'enterprise';
  since: string;
  orders: number;
  totalSpend: string;
  csat: number;
  interactions: number;
  lastInteractionAt: string;
  sentiment: Sentiment;
  recentActivity: readonly { label: string; at: string }[];
}

export interface MetricFixture {
  id: string;
  label: string;
  value: string;
  delta: string;
  deltaDirection: DeltaDirection;
  deltaPositive: boolean;
  icon: string;
  trend: readonly number[];
}

export interface TrendSeriesFixture {
  id: string;
  label: string;
  colorToken: 'accent' | 'green' | 'red' | 'amber';
  points: readonly number[];
}

export interface ChannelBreakdownFixture {
  channel: Channel;
  label: string;
  percentage: number;
}

export interface TopArticleFixture {
  id: string;
  title: string;
  category: string;
  uses: number;
  resolutionRate: number;
}

export interface KnowledgeArticleFixture {
  id: string;
  title: string;
  category: string;
  status: ArticleStatus;
  source: ArticleSource;
  updatedAt: string;
  indexed: boolean;
  excerpt: string;
}

export interface IntegrationFixture {
  id: string;
  name: string;
  description: string;
  icon: string;
  status: IntegrationStatus;
  actionLabel: 'Connect' | 'Manage' | 'Notify me';
}

export interface WorkspaceProfileFixture {
  name: string;
  domain: string;
  timezone: string;
  defaultLanguage: string;
}

export interface TeamMemberFixture {
  id: string;
  name: string;
  email: string;
  avatarInitials: string;
  role: 'owner' | 'admin' | 'manager' | 'agent' | 'viewer';
  status: 'active' | 'invited';
}

export interface UsageFixture {
  label: string;
  used: number;
  limit: number;
  unit: string;
}

export interface InvoiceFixture {
  id: string;
  period: string;
  amount: string;
  status: 'paid' | 'due';
}

export interface ApiKeyFixture {
  label: string;
  maskedValue: string;
  createdAt: string;
}

export interface SessionFixture {
  id: string;
  device: string;
  location: string;
  lastActiveAt: string;
  current: boolean;
}

export interface AlertFixture {
  title: string;
  description: string;
}
