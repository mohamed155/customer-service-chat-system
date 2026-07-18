//! Root OpenAPI 3.1 document for the backend.
//!
//! The composed `ApiDoc` is registered with `utoipa-axum`'s `OpenApiRouter`
//! at the call site in `router.rs`. The `Modify` impl adds the shared
//! `session_cookie` security scheme, the `/api/v1` server, and the tag
//! vocabulary used to group operations in the interactive UI.
//!
//! **API paths are no longer listed here.** Every `/api/v1` route is
//! registered through `utoipa_axum::routes!` in `router.rs`, so the
//! documented `OpenApi` is built from the route registration itself
//! (FR-012, FR-015 — T035). This struct carries the shared components, the
//! security scheme, the server, the tag vocabulary, and the operational
//! paths (`/health`, `/ready`, `/metrics`) — the latter are mounted as
//! plain axum routes in `build_app` (outside the `/api/v1` nest, with a
//! different state type) so they are still listed here.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{openapi, Modify, OpenApi};

/// Tag vocabulary used to group operations in the interactive UI.
///
/// Tags MUST stay in sync with the `tag = "..."` arguments on
/// `#[utoipa::path(...)]` annotations and with the inventory in
/// `specs/016-backend-swagger-docs/contracts/openapi-coverage.md`.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "AI Customer Service Platform API",
        version = "1.0.0",
        description = "REST API for the multi-tenant AI customer service platform. \
                      All authenticated endpoints require a valid `app_session` cookie."
    ),
    modifiers(&SecurityAddon, &ServerAddon, &TagDescriptionsAddon),
    // Operational paths (mounted outside `/api/v1` — see module docs).
    paths(
        observability::liveness,
        observability::metrics,
        crate::router::ready_handler,
    ),
    // ── Components ────────────────────────────────────────────────────────
    components(schemas(
        // Shared
        kernel::ErrorEnvelope,
        kernel::ErrorBody,
        kernel::ErrorDetail,
        // Identity
        identity::routes::LoginRequest,
        identity::Principal,
        // Tenancy — platform
        tenancy::routes::TenantSummary,
        tenancy::routes::PlatformTenantDetail,
        tenancy::routes::CreateTenantRequest,
        tenancy::routes::UpdateTenantRequest,
        tenancy::routes::MeResponse,
        tenancy::routes::MembershipSummary,
        // Members & invitations
        tenancy::members::TeamMemberResponse,
        tenancy::members::UpdateMemberPayload,
        tenancy::invitations::CreateInvitationPayload,
        tenancy::invitations::AcceptInvitationPayload,
        tenancy::invitations::InvitationResponse,
        tenancy::invitations::CreateInvitationResponse,
        tenancy::invitations::InvitationListItem,
        tenancy::invitations::InvitationDeliveryResponse,
        tenancy::invitations::PreviewInvitationResponse,
        tenancy::invitations::AcceptInvitationResponse,
        // Customers
        customers::model::CustomerListItem,
        customers::model::CustomerDetail,
        customers::model::ChannelIdentifier,
        customers::model::ChannelIdentifierInput,
        customers::model::CreateCustomerPayload,
        customers::model::UpdateCustomerPayload,
        customers::model::CustomerDetailResponse,
        customers::model::CustomerListResponse,
        customers::model::PaginationEnvelope,
        // Conversations
        conversations::model::ConversationStatus,
        conversations::model::MessageKind,
        conversations::model::ConversationStatusRef,
        conversations::model::Assignee,
        conversations::model::CustomerRef,
        conversations::model::LastMessagePreview,
        conversations::model::Participant,
        conversations::model::Conversation,
        conversations::model::ConversationDetail,
        conversations::model::Message,
        conversations::model::AddMessageResponse,
        conversations::model::CreateConversationPayload,
        conversations::model::CreateMessagePayload,
        conversations::model::AddMessagePayload,
        conversations::model::PatchConversationPayload,
        conversations::routes::Pagination,
        conversations::routes::ConversationsListResponse,
        conversations::routes::MessagesListResponse,
        conversations::routes::ConversationDetailResponse,
        conversations::routes::AddMessageResponseEnvelope,
        conversations::ConversationSummary,
        conversations::HistoryPagination,
        conversations::HistoryResponse,
        // Escalations
        escalations::model::RoutingReason,
        escalations::model::EscalationStatus,
        escalations::model::AvailabilityState,
        escalations::model::RequiredSkillRef,
        escalations::model::RoutingInfo,
        escalations::model::QueueEntryConversationRef,
        escalations::model::Escalation,
        escalations::model::QueueEntry,
        escalations::model::Skill,
        escalations::model::Availability,
        escalations::model::TeamMemberSkill,
        escalations::model::TeamMemberWithSkills,
        escalations::model::EscalatePayload,
        escalations::model::SetAvailabilityPayload,
        escalations::model::CreateSkillPayload,
        escalations::model::RenameSkillPayload,
        escalations::model::SetMemberSkillsPayload,
        escalations::model::EscalationAssignedEvent,
        escalations::model::EscalationQueuedEvent,
        escalations::model::EscalationRemovedEvent,
        escalations::model::AvailabilityChangedEvent,
        escalations::routes::QueueListResponse,
        escalations::routes::SkillListResponse,
        escalations::routes::MemberSkillsResponse,
        // Server composite handlers
        crate::handlers::TeamMemberWithSkills,
        // AI
        ai::model::ConfigPayload,
        ai::model::FallbackEntry,
        ai::model::AiConfigurationView,
        ai::model::CredentialView,
        ai::model::CredentialPayload,
        ai::model::TestConfigResult,
        ai::model::UsageDetailResponse,
        ai::usage::UsageListItem,
        ai::usage::UsageSummary,
        ai::usage::UsageDetailRow,
        ai::usage::Pagination,
        // Observability
        observability::health::HealthReport,
        observability::health::HealthStatus,
        observability::health::CheckResult,
        observability::health::CheckStatus,
    ))
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut openapi::OpenApi) {
        let components = openapi
            .components
            .get_or_insert_with(openapi::Components::new);
        components.add_security_scheme(
            "session_cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("app_session"))),
        );
    }
}

struct ServerAddon;

impl Modify for ServerAddon {
    fn modify(&self, openapi: &mut openapi::OpenApi) {
        openapi.servers = Some(vec![openapi::Server::new("/api/v1")]);
    }
}

struct TagDescriptionsAddon;

impl Modify for TagDescriptionsAddon {
    fn modify(&self, openapi: &mut openapi::OpenApi) {
        let tags = vec![
            ("auth", "Public authentication endpoints (login, logout)."),
            (
                "invitations",
                "Public invitation preview and acceptance endpoints.",
            ),
            (
                "identity",
                "Authenticated identity endpoints (current user).",
            ),
            (
                "platform-tenants",
                "Platform-internal tenant lifecycle management.",
            ),
            (
                "platform-ai",
                "Platform-internal AI provider and configuration.",
            ),
            ("tenant", "Authenticated tenant profile access."),
            ("customers", "Customer profile management within a tenant."),
            (
                "conversations",
                "Conversations and messages within a tenant.",
            ),
            (
                "escalations",
                "Escalations, queue, availability, and skills within a tenant.",
            ),
            ("members", "Tenant team members and invitations."),
            (
                "tenant-ai",
                "Tenant-scoped AI provider and configuration with usage telemetry.",
            ),
            (
                "tools",
                "Tenant-scoped tool definitions, policies, activity, and approvals.",
            ),
            (
                "ops",
                "Operational endpoints (liveness, readiness, metrics).",
            ),
        ];
        let mut out = Vec::with_capacity(tags.len());
        for (name, description) in tags {
            let mut tag = openapi::Tag::new(name);
            tag.description = Some(description.to_owned());
            out.push(tag);
        }
        openapi.tags = Some(out);
    }
}
