import { Permission } from '../authz/permissions';

export type TenantStatus = 'active' | 'suspended';

export type MembershipRole = 'owner' | 'admin' | 'manager' | 'agent' | 'viewer';

export type PlatformRole = 'super_admin' | 'developer' | 'sales' | 'support' | 'finance';

export type TenantPlan = 'trial' | 'starter' | 'professional' | 'enterprise';

export interface TenantSummary {
  readonly id: string;
  readonly name: string;
  readonly slug: string;
  readonly status: TenantStatus;
  readonly plan: TenantPlan;
}

export interface MembershipSummary {
  readonly tenantId: string;
  readonly tenantName: string;
  readonly tenantSlug: string;
  readonly role: MembershipRole;
  readonly permissions: Permission[];
}

export interface MeResponse {
  readonly id: string;
  readonly email: string;
  readonly displayName: string;
  readonly platformRole: PlatformRole | null;
  readonly platformPermissions: Permission[];
  readonly staffTenantPermissions: Permission[] | null;
  readonly memberships: MembershipSummary[];
}

export interface TenantDirectoryParams {
  readonly cursor?: string;
  readonly limit?: number;
  readonly q?: string;
}

export interface PlatformTenantDetail {
  readonly id: string;
  readonly name: string;
  readonly slug: string;
  readonly status: TenantStatus;
  readonly plan: TenantPlan;
  readonly contactName: string | null;
  readonly contactEmail: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface CreateTenantPayload {
  readonly name: string;
  readonly slug: string;
  readonly plan?: TenantPlan;
  readonly contactName?: string;
  readonly contactEmail?: string;
}

export interface UpdateTenantPayload {
  readonly name?: string;
  readonly slug?: string;
  readonly plan?: TenantPlan;
  readonly status?: TenantStatus;
  readonly contactName?: string | null;
  readonly contactEmail?: string | null;
}

export interface TenantDirectoryQuery {
  readonly q?: string;
  readonly status?: TenantStatus;
  readonly cursor?: string;
  readonly limit?: number;
}

export type MemberStatus = 'active' | 'disabled';

export interface TeamMember {
  readonly id: string;
  readonly userId: string;
  readonly displayName: string;
  readonly email: string;
  readonly role: MembershipRole;
  readonly status: MemberStatus;
  readonly joinedAt: string;
  readonly skills?: Skill[];
  readonly availability?: AvailabilityState;
}

export type InvitationStatus = 'pending' | 'accepted' | 'revoked' | 'expired';

export interface TenantInvitation {
  readonly id: string;
  readonly email: string;
  readonly role: MembershipRole;
  readonly status: InvitationStatus;
  readonly invitedByName: string;
  readonly createdAt: string;
  readonly expiresAt: string;
  readonly emailDeliveryStatus: InvitationDeliveryStatus;
}

export interface CreateInvitationPayload {
  readonly email: string;
  readonly role: MembershipRole;
}

export interface CreateInvitationResponse {
  readonly invitation: TenantInvitation;
  readonly acceptUrl: string;
  readonly emailSent: boolean;
  readonly emailDeliveryStatus: InvitationDeliveryStatus;
}

export type InvitationDeliveryStatus = 'unconfigured' | 'queued' | 'sent' | 'failed';

export interface InvitationDeliveryResponse {
  readonly emailDeliveryStatus: InvitationDeliveryStatus;
}

export interface InvitationPreview {
  readonly tenantName: string;
  readonly email: string;
  readonly role: MembershipRole;
  readonly expiresAt: string;
  readonly accountExists: boolean;
}

export interface AcceptInvitationRequest {
  readonly displayName?: string;
  readonly password?: string;
}

export interface PatchMemberPayload {
  readonly role?: MembershipRole;
  readonly status?: MemberStatus;
}

export interface TeamMemberQuery {
  readonly q?: string;
  readonly status?: MemberStatus;
  readonly cursor?: string;
  readonly limit?: number;
}

export interface InvitationQuery {
  readonly status?: InvitationStatus | 'expired';
  readonly cursor?: string;
  readonly limit?: number;
}

export interface ChannelIdentifier {
  readonly id: string;
  readonly channel: 'email' | 'phone' | 'web_chat' | 'whatsapp' | 'telegram';
  readonly identifier: string;
}

export interface Customer {
  readonly id: string;
  readonly displayName: string;
  readonly email: string | null;
  readonly phone: string | null;
  readonly channels: ChannelIdentifier['channel'][];
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly identifiers?: ChannelIdentifier[];
  readonly metadata?: Record<string, string>;
}

export interface CustomerDetail extends Omit<Customer, 'identifiers' | 'metadata'> {
  readonly identifiers: ChannelIdentifier[];
  readonly metadata: Record<string, string>;
}

export interface ConversationSummary {
  readonly id: string;
  readonly channel: ChannelIdentifier['channel'];
  readonly status: 'open' | 'pending' | 'resolved' | 'closed';
  readonly lastActivityAt: string;
  readonly createdAt: string;
}

export interface CreateCustomerPayload {
  readonly displayName: string;
  readonly email?: string;
  readonly phone?: string;
  readonly identifiers?: Omit<ChannelIdentifier, 'id'>[];
  readonly metadata?: Record<string, string>;
}

export interface UpdateCustomerPayload {
  readonly displayName?: string;
  readonly email?: string | null;
  readonly phone?: string | null;
  readonly identifiers?: Omit<ChannelIdentifier, 'id'>[];
  readonly metadata?: Record<string, string>;
}

// Wire DTOs matching the backend's snake_case serialization
export interface CustomerWire {
  readonly id: string;
  readonly display_name: string;
  readonly email: string | null;
  readonly phone: string | null;
  readonly channels: readonly string[];
  readonly created_at: string;
  readonly updated_at: string;
}

export interface CustomerDetailWire {
  readonly id: string;
  readonly display_name: string;
  readonly email: string | null;
  readonly phone: string | null;
  readonly channels: readonly string[];
  readonly identifiers: readonly ChannelIdentifierWire[];
  readonly metadata: Record<string, string>;
  readonly created_at: string;
  readonly updated_at: string;
}

export interface ChannelIdentifierWire {
  readonly id: string;
  readonly channel: string;
  readonly identifier: string;
}

export interface ConversationSummaryWire {
  readonly id: string;
  readonly channel: string;
  readonly status: string;
  readonly last_activity_at: string;
  readonly created_at: string;
}

export interface CreateCustomerPayloadWire {
  readonly display_name: string;
  readonly email?: string;
  readonly phone?: string;
  readonly identifiers?: readonly { channel: string; identifier: string }[];
  readonly metadata?: Record<string, string>;
}

export interface UpdateCustomerPayloadWire {
  readonly display_name?: string;
  readonly email?: string | null;
  readonly phone?: string | null;
  readonly identifiers?: readonly { channel: string; identifier: string }[];
  readonly metadata?: Record<string, string>;
}

export function customerFromWire(wire: CustomerWire): Customer {
  return {
    id: wire.id,
    displayName: wire.display_name,
    email: wire.email,
    phone: wire.phone,
    channels: [...wire.channels] as Customer['channels'],
    createdAt: wire.created_at,
    updatedAt: wire.updated_at,
  };
}

export function customerDetailFromWire(wire: CustomerDetailWire): CustomerDetail {
  return {
    id: wire.id,
    displayName: wire.display_name,
    email: wire.email,
    phone: wire.phone,
    channels: [...wire.channels] as CustomerDetail['channels'],
    identifiers: wire.identifiers.map((i) => ({
      id: i.id,
      channel: i.channel as ChannelIdentifier['channel'],
      identifier: i.identifier,
    })),
    metadata: { ...wire.metadata },
    createdAt: wire.created_at,
    updatedAt: wire.updated_at,
  };
}

export type ConversationStatus = 'open' | 'pending' | 'resolved' | 'closed';
export type MessageKind = 'customer' | 'reply' | 'note' | 'ai';

export interface Participant {
  readonly type: 'customer' | 'member';
  readonly id?: string;
  readonly membershipId?: string;
  readonly displayName: string;
  readonly active?: boolean;
}

export interface ConversationDetail extends Conversation {
  readonly participants: readonly Participant[];
  readonly aiHandling?: string | null;
  readonly awaitingAiDecision?: boolean;
}

export interface RequiredSkillRef {
  readonly id: string | null;
  readonly name: string;
}

export type EscalationStatus = 'queued' | 'assigned' | 'closed';

export type RoutingReason =
  'skill_match' | 'load_fallback' | 'manual_claim' | 'queue_auto' | 'manual_reassignment';

export interface RoutingInfo {
  readonly reason: RoutingReason;
  readonly matchedSkills: string[];
  readonly assignedMembershipId: string;
  readonly assignedAt: string;
}

export interface Escalation {
  readonly id: string;
  readonly conversationId: string;
  readonly reason: string;
  readonly requiredSkills: RequiredSkillRef[];
  readonly status: EscalationStatus;
  readonly routing: RoutingInfo | null;
  readonly escalatedAt: string;
  readonly closedAt: string | null;
}

export interface QueueEntry {
  readonly escalation: Escalation;
  readonly conversation: {
    readonly id: string;
    readonly channel: string;
    readonly customer: { readonly id: string; readonly name: string };
  };
  readonly waitingSeconds: number;
}

export interface Skill {
  readonly id: string;
  readonly name: string;
  readonly agentCount: number;
}

export type AvailabilityState = 'available' | 'away';

export interface Availability {
  readonly membershipId: string;
  readonly state: AvailabilityState;
  readonly stateChangedAt: string | null;
}

export interface ConversationDetailEscalation extends ConversationDetail {
  readonly escalation: Escalation | null;
}

export interface ConversationDetailEscalationWire extends ConversationDetailWire {
  readonly escalation: Escalation | null;
}

export interface EscalationAssignedEvent {
  readonly v: number;
  readonly escalationId: string;
  readonly conversationId: string;
  readonly reason: string;
  readonly routingReason: RoutingReason;
  readonly matchedSkills: string[];
  readonly assignedAt: string;
}

export interface EscalationQueuedEvent {
  readonly v: number;
  readonly escalationId: string;
  readonly conversationId: string;
  readonly escalatedAt: string;
  readonly requiredSkills: string[];
}

export interface EscalationRemovedEvent {
  readonly v: number;
  readonly escalationId: string;
  readonly cause: string;
}

export interface AvailabilityChangedEvent {
  readonly v: number;
  readonly membershipId: string;
  readonly state: AvailabilityState;
}

// AI conversation SSE event types (from contracts/ai-events-sse.md)
export interface AiMessageStarted {
  readonly conversationId: string;
  readonly generationId: string;
  readonly triggerMessageId: string;
  readonly startedAt: string;
}

export interface AiMessageDelta {
  readonly conversationId: string;
  readonly generationId: string;
  readonly text: string;
}

export interface AiMessageCompleted {
  readonly conversationId: string;
  readonly generationId: string;
  readonly message: Message;
}

export interface AiMessageSuperseded {
  readonly conversationId: string;
  readonly generationId: string;
  readonly reason: 'newer_message' | 'escalated';
}

export interface AiMessageFailed {
  readonly conversationId: string;
  readonly generationId: string;
  readonly category:
    'unavailable' | 'timeout' | 'rate_limited' | 'authentication' | 'invalid_request' | 'internal';
}

export interface Citation {
  readonly knowledgeItemId: string;
  readonly itemTitle: string;
  readonly passageText: string;
  readonly relevanceScore: number;
  readonly itemAvailable: boolean;
}

export interface Message {
  readonly id: string;
  readonly kind: MessageKind;
  readonly sender: {
    readonly type: 'customer' | 'member';
    readonly displayName: string;
    readonly membershipId?: string;
  };
  readonly loggedBy: { readonly membershipId: string; readonly displayName: string } | null;
  readonly body: string;
  readonly createdAt: string;
  readonly citations?: readonly Citation[];
  readonly confidence?: { readonly score: number; readonly band: 'high' | 'medium' | 'low' };
}

export interface AddMessageResponse {
  readonly message: Message;
  readonly conversation: { readonly status: ConversationStatus; readonly lastActivityAt: string };
}

export interface AddMessagePayload {
  readonly kind: MessageKind;
  readonly body: string;
}

export interface PatchConversationPayload {
  readonly status?: ConversationStatus;
  readonly assignedMembershipId?: string | null;
}

export interface CreateConversationPayload {
  readonly customerId: string;
  readonly channel: string;
  readonly message: { readonly body: string };
}
export type AssigneeFilter = 'me' | 'unassigned' | string;

export interface ConversationWire {
  readonly id: string;
  readonly customer: { readonly id: string; readonly display_name: string };
  readonly channel: string;
  readonly status: ConversationStatus;
  readonly assignee: {
    readonly membership_id: string;
    readonly display_name: string;
    readonly active: boolean;
  } | null;
  readonly last_message: { readonly kind: MessageKind; readonly preview: string } | null;
  readonly last_activity_at: string;
  readonly created_at: string;
}

export interface ParticipantWire {
  readonly type: string;
  readonly id?: string;
  readonly membership_id?: string;
  readonly display_name: string;
  readonly active?: boolean;
}

export interface ConversationDetailWire extends ConversationWire {
  readonly participants: readonly ParticipantWire[];
  readonly ai_handling?: string | null;
  readonly awaiting_ai_decision?: boolean;
}

export interface CitationWire {
  readonly knowledge_item_id: string;
  readonly item_title: string;
  readonly passage_text: string;
  readonly relevance_score: number;
  readonly item_available: boolean;
}

export interface MessageWire {
  readonly id: string;
  readonly kind: string;
  readonly sender: {
    readonly type: string;
    readonly display_name: string;
    readonly membership_id?: string;
  };
  readonly logged_by: { readonly membership_id: string; readonly display_name: string } | null;
  readonly body: string;
  readonly created_at: string;
  readonly citations?: readonly CitationWire[];
  readonly confidence?: { readonly score: number; readonly band: string };
}

export interface AddMessageResponseWire {
  readonly message: MessageWire;
  readonly conversation: { readonly status: ConversationStatus; readonly last_activity_at: string };
}

export interface AddMessagePayloadWire {
  readonly kind: string;
  readonly body: string;
}

export interface PatchConversationPayloadWire {
  readonly status?: string;
  readonly assigned_membership_id?: string | null;
}

export interface CreateConversationPayloadWire {
  readonly customer_id: string;
  readonly channel: string;
  readonly message: { readonly body: string };
}

export interface Conversation {
  readonly id: string;
  readonly customer: { readonly id: string; readonly displayName: string };
  readonly channel: string;
  readonly status: ConversationStatus;
  readonly assignee: {
    readonly membershipId: string;
    readonly displayName: string;
    readonly active: boolean;
  } | null;
  readonly lastMessage: { readonly kind: MessageKind; readonly preview: string } | null;
  readonly lastActivityAt: string;
  readonly createdAt: string;
}

export interface ConversationListQuery {
  readonly status?: ConversationStatus | 'all';
  readonly assignee?: string;
  readonly channel?: string;
  readonly escalated?: string;
  readonly cursor?: string;
  readonly limit?: number;
}

export function conversationFromWire(wire: ConversationWire): Conversation {
  return {
    id: wire.id,
    customer: {
      id: wire.customer.id,
      displayName: wire.customer.display_name,
    },
    channel: wire.channel,
    status: wire.status,
    assignee: wire.assignee
      ? {
          membershipId: wire.assignee.membership_id,
          displayName: wire.assignee.display_name,
          active: wire.assignee.active,
        }
      : null,
    lastMessage: wire.last_message
      ? {
          kind: wire.last_message.kind,
          preview: wire.last_message.preview,
        }
      : null,
    lastActivityAt: wire.last_activity_at,
    createdAt: wire.created_at,
  };
}

export function conversationSummaryFromWire(wire: ConversationSummaryWire): ConversationSummary {
  return {
    id: wire.id,
    channel: wire.channel as ConversationSummary['channel'],
    status: wire.status as ConversationSummary['status'],
    lastActivityAt: wire.last_activity_at,
    createdAt: wire.created_at,
  };
}

export function createPayloadToWire(payload: CreateCustomerPayload): CreateCustomerPayloadWire {
  return {
    display_name: payload.displayName,
    ...(payload.email ? { email: payload.email } : {}),
    ...(payload.phone ? { phone: payload.phone } : {}),
    ...(payload.identifiers && payload.identifiers.length > 0
      ? {
          identifiers: payload.identifiers.map((i) => ({
            channel: i.channel,
            identifier: i.identifier,
          })),
        }
      : {}),
    ...(payload.metadata && Object.keys(payload.metadata).length > 0
      ? { metadata: payload.metadata }
      : {}),
  };
}

export function updatePayloadToWire(payload: UpdateCustomerPayload): UpdateCustomerPayloadWire {
  return {
    ...('displayName' in payload ? { display_name: payload.displayName } : {}),
    ...('email' in payload ? { email: payload.email } : {}),
    ...('phone' in payload ? { phone: payload.phone } : {}),
    ...('identifiers' in payload
      ? {
          identifiers: payload.identifiers?.map((i) => ({
            channel: i.channel,
            identifier: i.identifier,
          })),
        }
      : {}),
    ...('metadata' in payload ? { metadata: payload.metadata } : {}),
  };
}

export function participantFromWire(wire: ParticipantWire): Participant {
  return {
    type: wire.type as Participant['type'],
    ...(wire.id ? { id: wire.id } : {}),
    ...(wire.membership_id ? { membershipId: wire.membership_id } : {}),
    displayName: wire.display_name,
    ...(wire.active !== undefined ? { active: wire.active } : {}),
  };
}

export function conversationDetailFromWire(wire: ConversationDetailWire): ConversationDetail {
  return {
    ...conversationFromWire(wire),
    participants: wire.participants.map(participantFromWire),
    aiHandling: wire.ai_handling ?? null,
    awaitingAiDecision: wire.awaiting_ai_decision ?? false,
  };
}

export function conversationDetailEscalationFromWire(
  wire: ConversationDetailEscalationWire,
): ConversationDetailEscalation {
  return {
    ...conversationDetailFromWire(wire),
    escalation: wire.escalation,
  };
}

export function citationFromWire(wire: CitationWire): Citation {
  return {
    knowledgeItemId: wire.knowledge_item_id,
    itemTitle: wire.item_title,
    passageText: wire.passage_text,
    relevanceScore: wire.relevance_score,
    itemAvailable: wire.item_available,
  };
}

export function messageFromWire(wire: MessageWire): Message {
  return {
    id: wire.id,
    kind: wire.kind as MessageKind,
    sender: {
      type: wire.sender.type as Message['sender']['type'],
      displayName: wire.sender.display_name,
      ...(wire.sender.membership_id ? { membershipId: wire.sender.membership_id } : {}),
    },
    loggedBy: wire.logged_by
      ? { membershipId: wire.logged_by.membership_id, displayName: wire.logged_by.display_name }
      : null,
    body: wire.body,
    createdAt: wire.created_at,
    ...(wire.citations ? { citations: wire.citations.map(citationFromWire) } : {}),
    ...(wire.confidence
      ? {
          confidence: {
            score: wire.confidence.score,
            band: wire.confidence.band as 'high' | 'medium' | 'low',
          },
        }
      : {}),
  };
}

export function addMessagePayloadToWire(payload: AddMessagePayload): AddMessagePayloadWire {
  return { kind: payload.kind, body: payload.body };
}

export function patchPayloadToWire(
  payload: PatchConversationPayload,
): PatchConversationPayloadWire {
  return {
    ...('status' in payload ? { status: payload.status } : {}),
    ...('assignedMembershipId' in payload
      ? { assigned_membership_id: payload.assignedMembershipId }
      : {}),
  };
}

export interface ToolRequest {
  readonly id: string;
  readonly toolName: string;
  readonly toolSource: 'builtin' | 'tenant';
  readonly arguments: unknown;
  readonly status: string;
  readonly approvalRequired: boolean;
  readonly chainIndex: number;
  readonly createdAt: string;
  readonly expiresAt?: string;
  readonly durationMs?: number;
  readonly result?: unknown;
  readonly error?: string;
  readonly decidedByDisplayName?: string;
}

export interface ToolRequestCreatedEvent {
  readonly id: string;
  readonly conversationId: string;
  readonly toolName: string;
  readonly toolSource: string;
  readonly arguments: unknown;
  readonly approvalRequired: boolean;
  readonly expiresAt?: string;
  readonly chainIndex: number;
  readonly createdAt: string;
}

export interface ToolRequestUpdatedEvent {
  readonly id: string;
  readonly conversationId: string;
  readonly status: string;
  readonly decidedByDisplayName?: string;
  readonly durationMs?: number;
  readonly hasResult: boolean;
  readonly error?: string;
}

export interface DecideToolRequestRequest {
  readonly decision: 'approve' | 'deny';
}

// Tool settings types (US4 — tenant tool settings page)
export interface BuiltinToolSetting {
  readonly name: string;
  readonly description: string;
  readonly classification: 'auto' | 'approval';
  readonly enabled: boolean;
  readonly requireApproval: boolean;
  readonly effectiveApproval: boolean;
}

export interface TenantDefinedTool {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly inputSchema: Record<string, unknown>;
  readonly endpointUrl: string;
  readonly hasCredential: boolean;
  readonly classification: 'auto' | 'approval';
  readonly enabled: boolean;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface CreateTenantToolPayload {
  readonly name: string;
  readonly description: string;
  readonly inputSchema: Record<string, unknown>;
  readonly endpointUrl: string;
  readonly credential?: string | null;
  readonly classification?: 'auto' | 'approval';
  readonly enabled?: boolean;
}

export interface UpdateTenantToolPayload {
  readonly name?: string;
  readonly description?: string;
  readonly inputSchema?: Record<string, unknown>;
  readonly endpointUrl?: string;
  readonly credential?: string | null;
  readonly classification?: 'auto' | 'approval';
  readonly enabled?: boolean;
}

export interface ToolsSettingsResponse {
  readonly builtin: readonly BuiltinToolSetting[];
  readonly tenantDefined: readonly TenantDefinedTool[];
}

export function createConversationPayloadToWire(
  payload: CreateConversationPayload,
): CreateConversationPayloadWire {
  return {
    customer_id: payload.customerId,
    channel: payload.channel,
    message: payload.message,
  };
}
