import {
  AlertFixture,
  ApiKeyFixture,
  InvoiceFixture,
  SessionFixture,
  TeamMemberFixture,
  UsageFixture,
  WorkspaceProfileFixture,
} from './fixture.models';

export const WORKSPACE_PROFILE: WorkspaceProfileFixture = {
  name: 'Helix Support Ops',
  domain: 'helix.example.com',
  timezone: 'Africa/Cairo',
  defaultLanguage: 'English',
};

export const TEAM_MEMBERS: readonly TeamMemberFixture[] = [
  {
    id: 'team-001',
    name: 'Nadia Farouk',
    email: 'nadia@helix.example.com',
    avatarInitials: 'NF',
    role: 'owner',
    status: 'active',
  },
  {
    id: 'team-002',
    name: 'Maya Chen',
    email: 'maya@helix.example.com',
    avatarInitials: 'MC',
    role: 'admin',
    status: 'active',
  },
  {
    id: 'team-003',
    name: 'Omar Hassan',
    email: 'omar@helix.example.com',
    avatarInitials: 'OH',
    role: 'manager',
    status: 'active',
  },
  {
    id: 'team-004',
    name: 'Priya Nair',
    email: 'priya@helix.example.com',
    avatarInitials: 'PN',
    role: 'agent',
    status: 'active',
  },
  {
    id: 'team-005',
    name: 'Leo Martin',
    email: 'leo@helix.example.com',
    avatarInitials: 'LM',
    role: 'viewer',
    status: 'invited',
  },
];

export const USAGE_FIXTURES: readonly UsageFixture[] = [
  { label: 'AI conversations', used: 1284, limit: 2000, unit: 'conversations' },
  { label: 'Knowledge articles', used: 84, limit: 120, unit: 'articles' },
  { label: 'Automation runs', used: 18420, limit: 25000, unit: 'runs' },
];

export const INVOICE_FIXTURES: readonly InvoiceFixture[] = [
  { id: 'inv-2026-07', period: 'July 2026', amount: '$1,240', status: 'due' },
  { id: 'inv-2026-06', period: 'June 2026', amount: '$1,188', status: 'paid' },
  { id: 'inv-2026-05', period: 'May 2026', amount: '$1,104', status: 'paid' },
];

export const API_KEY_FIXTURE: ApiKeyFixture = {
  label: 'Production workspace key',
  maskedValue: 'hx_live_••••_••••_••••_8F2A',
  createdAt: '2026-06-12T09:30:00.000Z',
};

export const SESSION_FIXTURES: readonly SessionFixture[] = [
  {
    id: 'session-001',
    device: 'Chrome on Windows',
    location: 'Cairo, EG',
    lastActiveAt: '2026-07-07T08:50:00.000Z',
    current: true,
  },
  {
    id: 'session-002',
    device: 'Safari on macOS',
    location: 'London, UK',
    lastActiveAt: '2026-07-06T17:20:00.000Z',
    current: false,
  },
  {
    id: 'session-003',
    device: 'Chrome on Android',
    location: 'Dubai, AE',
    lastActiveAt: '2026-07-05T11:45:00.000Z',
    current: false,
  },
];

export const OVERVIEW_ALERT: AlertFixture = {
  title: 'Elevated AI provider latency',
  description:
    'Responses are still within SLA, but escalation monitoring is tightened for WhatsApp and Telegram queues.',
};
