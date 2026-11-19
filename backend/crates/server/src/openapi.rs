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
                      All authenticated endpoints require a valid `app_session` cookie.\n\n\
                      **Errors.** An error answers in `application/problem+json` (RFC 9457, \
                      `ProblemDetails`) when the request's `Accept` header asks for it, for \
                      example `Accept: application/problem+json, application/json;q=0.5`. \
                      Any other request still gets the legacy `application/json` envelope \
                      (`ErrorEnvelope`), which is deprecated: it carries `Deprecation` and \
                      `Link` headers and will be removed (ADR-012, gap A2)."
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
        kernel::ProblemDetails,
        kernel::ProblemFieldError,
        // Identity
        identity::routes::LoginRequest,
        identity::Principal,
        tenancy::signup::SignupRequest,
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
        conversations::model::WidgetInstanceRef,
        conversations::model::Participant,
        conversations::model::Conversation,
        conversations::model::ConversationDetail,
        conversations::model::Message,
        conversations::model::AddMessageResponse,
        conversations::model::TenantFeedbackDto,
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
        // Feedback
        feedback::model::SubmitFeedbackPayload,
        feedback::model::WidgetFeedbackDto,
        feedback::model::WidgetFeedbackResponse,
        feedback::model::WidgetFeedbackResponseData,
        feedback::model::PendingFeedbackDto,
        feedback::model::PendingFeedbackResponse,
        feedback::model::FeedbackSummaryDto,
        // Analytics
        analytics::model::DateRangeDto,
        analytics::model::ChannelBreakdownItem,
        analytics::model::AnalyticsSummaryDto,
        analytics::model::TimeseriesDay,
        analytics::model::AnalyticsTimeseriesDto,
        // Audit (spec 026)
        audit::model::AuditActorDto,
        audit::model::AuditEntryDto,
        audit::model::AuditPagination,
        audit::model::AuditListResponse,
        // Billing (US1 / US3)
        billing::model::PlanDto,
        billing::model::PeriodDto,
        billing::model::MetricUsageDto,
        billing::model::UsageDto,
        billing::model::BillingOverviewDto,
        billing::model::PlanListResponse,
        billing::model::CreatePlanRequest,
        billing::model::UpdatePlanRequest,
        billing::model::SubscriptionRowDto,
        billing::model::SubscriptionListResponse,
        billing::model::PutSubscriptionRequest,
        billing::model::SubscriptionDto,
        // Notifications (spec 027)
        notifications::model::NotificationDto,
        notifications::model::NotificationActorDto,
        notifications::model::NotificationListResponse,
        notifications::model::UnreadCountResponse,
        notifications::model::MarkedResponse,
        // Integrations (spec 028)
        integrations::model::ConfigFieldDto,
        integrations::model::IntegrationListItemDto,
        integrations::model::IntegrationListResponse,
        integrations::model::IntegrationSecretRefDto,
        integrations::model::IntegrationConnectionDto,
        integrations::model::IntegrationDetailDto,
        integrations::model::ConnectPayload,
        integrations::model::UpdateConfigPayload,
        integrations::model::IntegrationEventDto,
        integrations::model::IntegrationEventListResponse,
        integrations::model::PaginationInfo,
        // Widgets
        widgets::model::PublicWidgetConfigDto,
        widgets::model::PublicWidgetConfigResponse,
        widgets::model::CreateSessionPayload,
        widgets::model::SessionResponse,
        widgets::model::SessionResponseDto,
        widgets::model::SendMessagePayload,
        widgets::model::WidgetMessageDto,
        widgets::model::WidgetConversationDto,
        widgets::model::WidgetConversationData,
        widgets::model::WidgetConversationResponse,
        widgets::model::WidgetMessageResponse,
        widgets::model::WidgetMessageResponseData,
        widgets::model::WidgetInstanceDto,
        widgets::model::CreateWidgetInstancePayload,
        widgets::model::UpdateWidgetInstancePayload,
        widgets::model::WidgetSnippetResponse,
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
                "widget-public",
                "Public widget endpoints (config, sessions). No authentication required.",
            ),
            (
                "widgets",
                "Tenant-scoped widget instance management (admin CRUD).",
            ),
            (
                "ops",
                "Operational endpoints (liveness, readiness, metrics).",
            ),
            (
                "notifications",
                "Tenant-scoped notification inbox (list, unread count, mark read).",
            ),
            (
                "platform-analytics",
                "Platform-internal cross-tenant business metrics.",
            ),
            (
                "platform-health",
                "Platform-internal system health and AI provider diagnostics.",
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

/// The statuses an operation documents as errors: `4xx` and `5xx`.
fn is_error_status(status: &str) -> bool {
    status.starts_with('4') || status.starts_with('5')
}

/// Whether a media type's schema is a reference to `name`.
fn references_schema(content: &openapi::content::Content, name: &str) -> bool {
    content
        .schema
        .as_ref()
        .and_then(|schema| serde_json::to_value(schema).ok())
        .and_then(|schema| {
            schema
                .get("$ref")
                .and_then(|r| r.as_str())
                .map(str::to_owned)
        })
        .is_some_and(|reference| reference == format!("#/components/schemas/{name}"))
}

/// Describes the two error formats on every documented error response (ADR-012, gap A2).
///
/// The handlers document their errors with `body = kernel::ErrorEnvelope`, which is the legacy
/// envelope. The server answers in problem+json instead when the request's `Accept` header asks
/// for it (`error_format_middleware`), so each such response gets a second media type,
/// `application/problem+json` with `ProblemDetails`, and the legacy `application/json` is marked
/// deprecated. It is done once here, on the finished document, because the paths of the router are
/// merged after [`ApiDoc`]'s own modifiers have run, and annotating hundreds of responses by hand
/// would drift.
pub fn describe_error_formats(openapi: &mut openapi::OpenApi) {
    for item in openapi.paths.paths.values_mut() {
        let operations = [
            &mut item.get,
            &mut item.put,
            &mut item.post,
            &mut item.delete,
            &mut item.options,
            &mut item.head,
            &mut item.patch,
            &mut item.trace,
        ];
        for operation in operations.into_iter().flatten() {
            for (status, response) in operation.responses.responses.iter_mut() {
                if !is_error_status(status) {
                    continue;
                }
                let openapi::RefOr::T(response) = response else {
                    continue;
                };
                let legacy_is_envelope = matches!(
                    response.content.get("application/json"),
                    Some(openapi::RefOr::T(content)) if references_schema(content, "ErrorEnvelope")
                );
                if !legacy_is_envelope {
                    continue;
                }
                if let Some(openapi::RefOr::T(legacy)) =
                    response.content.get_mut("application/json")
                {
                    legacy.description = Some(
                        "Deprecated. The legacy error envelope, sent when the request does not \
                         ask for `application/problem+json` (ADR-012, gap A2). The response carries \
                         `Deprecation` and `Link` headers."
                            .to_owned(),
                    );
                    let mut extensions = openapi::extensions::Extensions::default();
                    extensions.insert("x-deprecated".to_owned(), serde_json::Value::Bool(true));
                    legacy.extensions = Some(extensions);
                }
                let mut problem = openapi::content::Content::new(Some(
                    openapi::Ref::from_schema_name("ProblemDetails"),
                ));
                problem.description = Some(
                    "RFC 9457 Problem Details, sent when the request's `Accept` header asks for \
                     `application/problem+json`."
                        .to_owned(),
                );
                // First, so a viewer that shows the first media type shows the one to use.
                response.content.shift_insert(
                    0,
                    kernel::PROBLEM_JSON.to_owned(),
                    openapi::RefOr::T(problem),
                );
            }
        }
    }
}
