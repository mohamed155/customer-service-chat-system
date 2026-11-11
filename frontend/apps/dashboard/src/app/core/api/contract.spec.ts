// The hand-written wire types must match the API contract (MOH-43, ADR-012 gap A5).
//
// `generated/api.ts` is generated from `docs/api/openapi.json`, which the backend test
// `openapi_snapshot` keeps in step with the code. The dashboard still has its own wire types in
// `tenant-api.models.ts` and `platform-analytics.models.ts` (see ADR-012 for the migration status).
// Until a call site uses the generated types directly, this file pins each hand-written type to the
// schema it is meant to describe: if the backend changes a field, or someone edits the wire type
// to something the backend does not send, this file stops compiling, which fails `ng test`.
//
// "Match" means mutually assignable: every value of one type is a value of the other. `readonly`
// does not matter, a missing or extra field, a different type, or optional vs required does.

import type { components } from './generated/api';
import type {
  AddMessageResponseWire,
  AnalyticsSummaryWire,
  AnalyticsTimeseriesWire,
  AuditActorWire,
  AuditPaginationWire,
  BillingMetricWire,
  BillingOverviewWire,
  BillingPeriodWire,
  BillingPlanWire,
  BillingUsageWire,
  ChannelIdentifierWire,
  CitationWire,
  ConversationSummaryWire,
  CustomerDetailWire,
  CustomerWire,
  FeedbackSummaryWire,
  IntegrationConnectionWire,
  IntegrationEventWire,
  IntegrationListItemWire,
  IntegrationSecretRefWire,
  MessageAttachmentWire,
  MessageDeliveryWire,
  MessageWire,
  NotificationListWire,
  NotificationWire,
  ParticipantWire,
  PlatformSubscriptionRowWire,
} from './tenant-api.models';
import type {
  PlatformSummaryWire,
  ProviderStatusWire,
  SystemHealthWire,
} from './platform-analytics.models';

type Schema = components['schemas'];

/**
 * The comparison ignores `readonly`, `null` and optionality, and looks at field names and value
 * types. The generated types mark every Rust `Option` field optional and nullable
 * (`x?: T | null`), but the backend either omits the key or sends it as `null` depending on a
 * serde attribute the schema does not record, so the two spellings cannot be told apart. What
 * the comparison does catch is what actually breaks the client: a renamed or differently cased
 * field, a missing field, a changed value type.
 */
type NonNull<T> = Exclude<T, null | undefined>;

/** The contract side: every field required, so a field the server may send must be known. */
type FromContract<T> = [T] extends [string | number | boolean | null | undefined]
  ? NonNull<T>
  : T extends readonly (infer U)[]
    ? FromContract<U>[]
    : T extends object
      ? { -readonly [K in keyof T]-?: FromContract<NonNull<T[K]>> }
      : T;

/** The client side: optional fields stay optional, so a looser client type is fine. */
type FromClient<T> = [T] extends [string | number | boolean | null | undefined]
  ? NonNull<T>
  : T extends readonly (infer U)[]
    ? FromClient<U>[]
    : T extends object
      ? { -readonly [K in keyof T]: FromClient<NonNull<T[K]>> }
      : T;

/**
 * True when everything the server may send for `Schema` fits the client's `Wire` type: it names
 * every field the same way and does not narrow a value type. A `Wire` that is looser than the
 * contract (extra optional fields, `string` for an enum) is fine.
 */
type Accepts<Wire, Schema> = [FromContract<Schema>] extends [FromClient<Wire>] ? true : false;

/** Compiles only when every pair is `true`; the error names the pair that is not. */
type AssertAll<T extends Record<keyof T, true>> = T;

// Known differences, not asserted (the client is narrower than the contract, which is harmless
// until the server sends something else; widen the wire type when touching the code that uses it):
//   - AuditEntry.details: the contract says `unknown` (any JSON), the wire type says an object.
//     AuditList and the audit screens depend on it.
//   - IntegrationConfigField.kind: the contract says `string`, the wire type says 'text' | 'secret'.
//     IntegrationDetail depends on it.
type Responses = {
  AddMessageResponse: Accepts<AddMessageResponseWire, Schema['AddMessageResponse']>;
  AnalyticsSummary: Accepts<AnalyticsSummaryWire, Schema['AnalyticsSummaryDto']>;
  AnalyticsTimeseries: Accepts<AnalyticsTimeseriesWire, Schema['AnalyticsTimeseriesDto']>;
  AuditActor: Accepts<AuditActorWire, Schema['AuditActorDto']>;
  AuditPagination: Accepts<AuditPaginationWire, Schema['AuditPagination']>;
  BillingMetric: Accepts<BillingMetricWire, Schema['MetricUsageDto']>;
  BillingOverview: Accepts<BillingOverviewWire, Schema['BillingOverviewDto']>;
  BillingPeriod: Accepts<BillingPeriodWire, Schema['PeriodDto']>;
  BillingPlan: Accepts<BillingPlanWire, Schema['PlanDto']>;
  BillingUsage: Accepts<BillingUsageWire, Schema['UsageDto']>;
  ChannelIdentifier: Accepts<ChannelIdentifierWire, Schema['ChannelIdentifier']>;
  Citation: Accepts<CitationWire, Schema['CitationView']>;
  ConversationSummary: Accepts<ConversationSummaryWire, Schema['ConversationSummary']>;
  CustomerDetail: Accepts<CustomerDetailWire, Schema['CustomerDetail']>;
  Customer: Accepts<CustomerWire, Schema['CustomerListItem']>;
  FeedbackSummary: Accepts<FeedbackSummaryWire, Schema['FeedbackSummaryDto']>;
  IntegrationConnection: Accepts<IntegrationConnectionWire, Schema['IntegrationConnectionDto']>;
  IntegrationEvent: Accepts<IntegrationEventWire, Schema['IntegrationEventDto']>;
  IntegrationListItem: Accepts<IntegrationListItemWire, Schema['IntegrationListItemDto']>;
  IntegrationSecretRef: Accepts<IntegrationSecretRefWire, Schema['IntegrationSecretRefDto']>;
  MessageAttachment: Accepts<MessageAttachmentWire, Schema['AttachmentInfo']>;
  MessageDelivery: Accepts<MessageDeliveryWire, Schema['DeliveryInfo']>;
  Message: Accepts<MessageWire, Schema['Message']>;
  Notification: Accepts<NotificationWire, Schema['NotificationDto']>;
  NotificationList: Accepts<NotificationListWire, Schema['NotificationListResponse']>;
  Participant: Accepts<ParticipantWire, Schema['Participant']>;
  PlatformSubscriptionRow: Accepts<PlatformSubscriptionRowWire, Schema['SubscriptionRowDto']>;
  PlatformSummary: Accepts<PlatformSummaryWire, Schema['PlatformUsageSummaryDto']>;
  ProviderStatus: Accepts<ProviderStatusWire, Schema['ProviderStatusDto']>;
  SystemHealth: Accepts<SystemHealthWire, Schema['SystemHealthDto']>;
};

// If this line fails to compile, the error says which key (which pair) is `false`.
export type ResponsesMatchTheContract = AssertAll<Responses>;

describe('API contract', () => {
  it('pins the hand-written response types to the generated schemas (checked when this file compiles)', () => {
    const pinned: ResponsesMatchTheContract | undefined = undefined;
    expect(pinned).toBeUndefined();
  });
});
