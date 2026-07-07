# Data Model: Helix Admin Dashboard Visual System

**Feature**: 003-helix-dashboard-visuals | **Date**: 2026-07-06

All entities are **fixture types** (visual rendering only — no persistence, no backend I/O). They live in `frontend/apps/dashboard/src/app/shared/fixtures/fixture.models.ts` as readonly interfaces/union types, with `const` fixture data in sibling `*.fixtures.ts` files (see research.md R10). Field vocabulary stays compatible with the spec-001 REST contract so later backend wiring is a data-source swap.

## Shared unions

```ts
type Channel = 'web' | 'whatsapp' | 'telegram' | 'mobile-sdk';   // mobile-sdk: overview breakdown only
type ConversationStatus = 'open' | 'escalated' | 'closed';
type Sentiment = 'positive' | 'neutral' | 'angry';
type MessageAuthor = 'customer' | 'ai' | 'human' | 'system';
type ArticleStatus = 'draft' | 'published' | 'archived';
type ArticleSource = 'article' | 'faq' | 'pdf' | 'url';
type IntegrationStatus = 'connected' | 'not-connected' | 'coming-soon';
type DeltaDirection = 'up' | 'down';
```

## ConversationFixture (`conversation.fixtures.ts`)

| Field | Type | Notes |
|-------|------|-------|
| `id` | `string` | unique within fixtures |
| `customerId` | `string` | → CustomerFixture.id |
| `channel` | `Channel` | web / whatsapp / telegram only |
| `status` | `ConversationStatus` | fixtures MUST cover all three |
| `sentiment` | `Sentiment` | fixtures MUST cover all three |
| `snippet` | `string` | list preview line |
| `updatedAt` | `string` (ISO) | rendered as relative time |
| `unread` | `boolean` | bold/list dot styling |
| `messages` | `readonly MessageFixture[]` | ordered timeline |

**Validation (by fixture design + tests)**: ≥8 conversations; every channel × status combination represented at least once; each conversation has ≥3 messages including at least one AI message; escalated conversations include a `system` event message.

### MessageFixture

| Field | Type | Notes |
|-------|------|-------|
| `id` | `string` | |
| `author` | `MessageAuthor` | `system` renders as inline event row, not bubble |
| `body` | `string` | |
| `createdAt` | `string` (ISO) | |
| `aiConfidence?` | `number` (0–1) | AI messages only → ai-confidence-badge |
| `citations?` | `readonly string[]` | knowledge article titles → knowledge-citation |

## CustomerFixture (`customer.fixtures.ts`)

| Field | Type | Notes |
|-------|------|-------|
| `id` | `string` | referenced by conversations |
| `name` / `email` | `string` | |
| `avatarInitials` | `string` | avatar component input (no image assets) |
| `channel` | `Channel` | primary channel |
| `tier` | `'free' \| 'pro' \| 'enterprise'` | badge |
| `since` | `string` (ISO date) | "customer since" |
| `orders` | `number` | |
| `totalSpend` | `string` | preformatted display value (e.g., "$4,320") |
| `csat` | `number` (0–100) | |
| `interactions` | `number` | conversation count display |
| `lastInteractionAt` | `string` (ISO) | |
| `sentiment` | `Sentiment` | profile panel |
| `recentActivity` | `readonly { label: string; at: string }[]` | customer sidebar list |

**Validation**: every `ConversationFixture.customerId` resolves to a customer (unit-tested referential check).

## Metric & chart fixtures (`analytics.fixtures.ts`)

### MetricFixture (used by Overview + Analytics metric cards)

| Field | Type | Notes |
|-------|------|-------|
| `id` | `string` | |
| `label` | `string` | e.g., "Resolved by AI" |
| `value` | `string` | preformatted ("1,284", "92%", "38s") |
| `delta` | `string` | preformatted ("+12.4%") |
| `deltaDirection` | `DeltaDirection` | green up / red down styling |
| `deltaPositive` | `boolean` | whether direction is good (escalation ↓ = good) |
| `icon` | `string` | Taiga icon name (`@tui.…`) |
| `trend` | `readonly number[]` | sparkline series (≥8 points) |

### TrendSeriesFixture (Overview main chart, Analytics charts)

| Field | Type |
|-------|------|
| `id` / `label` | `string` |
| `colorToken` | `'accent' \| 'green' \| 'red' \| 'amber'` (maps to `--app-*`) |
| `points` | `readonly number[]` |

### ChannelBreakdownFixture

| Field | Type | Notes |
|-------|------|-------|
| `channel` | `Channel` | all four incl. mobile-sdk |
| `label` | `string` | display name |
| `percentage` | `number` | four entries sum to 100 (tested) |

### TopArticleFixture (Analytics table)

`{ id, title, category, uses: number, resolutionRate: number }`

## KnowledgeArticleFixture (`knowledge.fixtures.ts`)

| Field | Type | Notes |
|-------|------|-------|
| `id` / `title` | `string` | |
| `category` | `string` | drives category filter options |
| `status` | `ArticleStatus` | all three represented |
| `source` | `ArticleSource` | all four represented |
| `updatedAt` | `string` (ISO) | "last updated" metadata |
| `indexed` | `boolean` | re-index status indicator |
| `excerpt` | `string` | card body |

## IntegrationFixture (`integration.fixtures.ts`)

| Field | Type | Notes |
|-------|------|-------|
| `id` / `name` / `description` | `string` | exactly the 8 named integrations |
| `icon` | `string` | Taiga icon name placeholder |
| `status` | `IntegrationStatus` | mix of all three |
| `actionLabel` | `'Connect' \| 'Manage' \| 'Notify me'` | derived from status |

## Settings fixtures (`settings.fixtures.ts`)

- **WorkspaceProfileFixture**: `{ name, domain, timezone, defaultLanguage }`
- **TeamMemberFixture**: `{ id, name, email, avatarInitials, role: 'owner'|'admin'|'manager'|'agent'|'viewer', status: 'active'|'invited' }` (roles mirror constitution tenant roles)
- **UsageFixture**: `{ label, used: number, limit: number, unit: string }` → usage bars
- **InvoiceFixture**: `{ id, period, amount: string, status: 'paid'|'due' }`
- **ApiKeyFixture**: `{ label, maskedValue, createdAt }` (masked string only — never a real-looking secret)
- **SessionFixture**: `{ id, device, location, lastActiveAt, current: boolean }`
- **SidebarUserFixture**: `{ name, role, company, avatarInitials }` (sidebar footer)
- **AlertFixture** (Overview banner): `{ title, description }`

## UI state (not fixtures)

### Global — existing `appUi` NgRx slice (reused unchanged)

```ts
interface AppUiState {
  themeMode: 'light' | 'dark' | 'system';   // persisted to localStorage (spec 002)
  sidebarCollapsed: boolean;                 // not persisted
}
```

Transitions: `themeModeChanged` (topbar cycle light→dark→system; Settings segmented control), `sidebarToggled`, `sidebarCollapsedSet` (viewport breakpoint effect — existing `LayoutStore`).

### Feature-local SignalStores (component-provided)

| Store | State | Computed | Transitions |
|-------|-------|----------|-------------|
| `ConversationsStore` | `selectedId: string \| null`, `statusFilter: ConversationStatus \| 'all'` | `filteredConversations`, `selectedConversation`, `selectedCustomer` | `select(id)`, `setFilter(f)`; initial `selectedId` = first fixture; if filter hides selection, selection moves to first visible (or empty state) |
| `AiAgentStore` | `activeTab: 'behavior' \| 'prompt' \| 'escalation' \| 'testing'` (init `behavior`) | — | `setTab(t)` |
| `SettingsStore` | `activeTab: 'general' \| 'team' \| 'billing' \| 'api-keys' \| 'security'` (init `general`) | — | `setTab(t)` |

### Component-temporary `signal()` state

Overview alert dismissed flag (in-memory, session-only), analytics date-range/channel selections, knowledge-base search text + category filter, customers search text, auth OTP input focus index, conversations customer-panel visibility on narrow viewports.
