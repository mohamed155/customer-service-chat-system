# Customer personal data inventory

This inventory covers tenant-scoped customer records and their operational copies.
Actions are the proposed MOH-103 disposition. RETAIN preserves structural data or
data outside the customer record's scope. A retained pseudonymous reference may
still be personal if it can be linked back to the person. Free text, opaque
payloads and immutable copies require the decisions listed below.

## PostgreSQL

The schema review follows migrations through 0099, including the composite
foreign-key rollout in 0092–0094. No migration creates or attaches partitions.
Migration 0080 drops `revoked_sessions`. `_sqlx_migrations` is runner metadata
with no application records. The `xtenant` schema contains privileged functions.

| Table | Owner | Customer data or potentially personal columns | Subject linkage within tenant | Action | Notes and schema evidence |
| --- | --- | --- | --- | --- | --- |
| `agent_availability` | escalations | None attributed | None | RETAIN | Staff membership and state. [Migration 0036](../../backend/migrations/0036_agent_availability.sql#L7) |
| `agent_avatar_uploads` | ai | bytes if staff uploaded customer imagery | agent_id | RETAIN | Tenant agent artwork is stored in PostgreSQL, including old soft-deleted versions. Review confirmed customer imagery separately. [Migration 0041](../../backend/migrations/0041_agent_configurations.sql#L56) |
| `agent_configurations` | ai | name, system_prompt, business_rules, escalation_rules if customer data was pasted | Tenant only | RETAIN | Editorial configuration needs provenance review for confirmed copies. [Migration 0041](../../backend/migrations/0041_agent_configurations.sql#L6) |
| `agent_prompt_versions` | ai | content, change_note if customer data was pasted | prompt_id | RETAIN | Immutable versions use forbid_mutation. Decide how to handle confirmed copies before promising full erasure. [Migration 0045](../../backend/migrations/0045_agent_prompts.sql#L36) |
| `agent_prompts` | ai | None attributed | None | RETAIN | Version root and active version pointer. [Migration 0045](../../backend/migrations/0045_agent_prompts.sql#L8) |
| `agent_skills` | escalations | None attributed | None | RETAIN | Staff membership and routing skill associations. [Migration 0035](../../backend/migrations/0035_agent_skills.sql#L40) |
| `ai_configurations` | ai | None attributed | None | RETAIN | Provider settings, defaults and content-capture switch. Captured content is stored in usage records. [Migration 0038](../../backend/migrations/0038_ai_configurations.sql#L6) |
| `ai_credentials` | ai | None attributed | None | RETAIN | Encrypted provider credentials and hints. [Migration 0039](../../backend/migrations/0039_ai_credentials.sql#L6) |
| `ai_generations` | ai | request_id (correlation) | conversation_id or trigger_message_id or response_message_id | ANONYMIZE | Keep usage link, outcome and numeric measures. Clear provider correlation after resolving usage. Content is in messages, citations, tools or usage. [Migration 0048](../../backend/migrations/0048_ai_conversation_engine.sql#L8) |
| `ai_usage_records` | ai | request_content, response_content, request_id | Generation usage_record_id and proven request correlation | ANONYMIZE | Keep usage, token counts and latency. Some captured rows lack a proven subject link. Resolve those before claiming completeness. [Migration 0040](../../backend/migrations/0040_ai_usage_records.sql#L5) |
| `audit_logs` | audit | details and resource_id can carry personal content | Tenant, typed customer/conversation/escalation resource, other free text | RETAIN | Append-only security and accountability records are retained. Older rows may contain staff free text, and customer IDs are pseudonymous. Keep customer personal data out of platform access reasons. [Migration 0006](../../backend/migrations/0006_audit_logs.sql#L1) |
| `auth_sessions` | identity | None attributed | None | RETAIN | Identity user, session identifiers, IP and user agent. Separate identity scope. [Migration 0079](../../backend/migrations/0079_auth_sessions_refresh_tokens.sql#L16) |
| `billing_plans` | billing | None attributed | None | RETAIN | Product pricing, allowances and status. [Migration 0059](../../backend/migrations/0059_billing_foundation.sql#L1) |
| `conversation_feedback` | feedback | comment, widget_session_id | conversation_id and widget_session_id | ANONYMIZE | Keep rating, submission time and channel. Clear comment and widget link. [Migration 0051](../../backend/migrations/0051_customer_feedback.sql#L14) |
| `conversations` | conversations | customer_id (pseudonymous link) | customer_id | RETAIN | Keep original channel, status and timestamps for analytics. Hide tombstones from operational reads. [Migration 0026](../../backend/migrations/0026_conversations.sql#L1) |
| `customer_channel_identifiers` | customers | identifier | customer_id | ERASE | Remove active and soft-deleted aliases, including external channel identifiers. [Migration 0025](../../backend/migrations/0025_customers.sql#L32) |
| `customers` | customers | display_name, email, phone, metadata | Tenant and customer primary key | ANONYMIZE | Keep tombstone. Use a fixed nonempty name and empty JSON object. Clear email and phone. [Migration 0025](../../backend/migrations/0025_customers.sql#L3) |
| `escalations` | escalations | reason, required_skill_names, matched_skill_names | conversation_id | ANONYMIZE | Keep existence and routing statistics. Scrub free text and copied labels. [Migration 0037](../../backend/migrations/0037_escalations.sql#L37) |
| `integration_catalog` | integrations | None attributed | None | RETAIN | Integration definitions and schemas. [Migration 0056](../../backend/migrations/0056_integrations_foundation.sql#L6) |
| `integration_connections` | integrations | config if customer data was pasted | Tenant and integration connection | RETAIN | Operational configuration, not linked to a customer. Keep connection credentials. [Migration 0056](../../backend/migrations/0056_integrations_foundation.sql#L20) |
| `integration_events` | integrations | None found | Connection and optional staff actor | RETAIN | Constrained event, outcome and reason codes. Raw delivery is stored separately. [Migration 0056](../../backend/migrations/0056_integrations_foundation.sql#L91) |
| `integration_secrets` | integrations | None attributed | None | RETAIN | Encrypted connection credentials and hints. [Migration 0056](../../backend/migrations/0056_integrations_foundation.sql#L51) |
| `integration_webhook_deliveries` | integrations | payload | Structured customer, conversation or message UUID within the tenant | ANONYMIZE | Export and scrub singly attributable deliveries using any spelling PostgreSQL's `uuid` input accepts: an optional matching `{}` pair around 32 hexadecimal digits, with an optional single hyphen after each group of four digits. Other forms such as a `urn:uuid:` prefix are not accepted, so those payloads are retained as unattributable. The `scrub_erasing_customer_webhook_delivery` trigger blanks late payloads for pending or erased subjects. Retain unattributed and mixed-subject JSON. [Table](../../backend/migrations/0056_integrations_foundation.sql#L73), [trigger](../../backend/migrations/0100_webhook_delivery_privacy_fence.sql) |
| `knowledge_categories` | knowledge | name if copied from customer data | Tenant only | RETAIN | Editorial configuration. Subject linkage is absent, so use reviewed attribution for confirmed copies. [Migration 0046](../../backend/migrations/0046_knowledge_base.sql#L6) |
| `knowledge_chunks` | knowledge | content, embedding and content_hash if the item contains customer data | item_id | RETAIN | Derived from editorial knowledge. Purge and rebuild index for an affected item. [Migration 0047](../../backend/migrations/0047_embeddings_rag.sql#L19) |
| `knowledge_documents` | knowledge | original_filename and stored document, if personal | item_id | RETAIN | Editorial upload. Handle confirmed customer copies with the owning item and object. [Migration 0046](../../backend/migrations/0046_knowledge_base.sql#L70) |
| `knowledge_index_state` | knowledge | failure_reason if it contains source content | item_id | RETAIN | Derived operational state. Scrub confirmed copies with the source item. [Migration 0047](../../backend/migrations/0047_embeddings_rag.sql#L42) |
| `knowledge_item_tags` | knowledge | tag if copied from customer data | item_id | RETAIN | Editorial label. Include confirmed copies in knowledge review. [Migration 0046](../../backend/migrations/0046_knowledge_base.sql#L81) |
| `knowledge_items` | knowledge | title, body if they contain customer data | Tenant and editorial source | RETAIN | Shared knowledge corpus. Confirmed customer content requires targeted editorial removal and reindexing. [Migration 0046](../../backend/migrations/0046_knowledge_base.sql#L31) |
| `login_lockout_notifications` | identity | None attributed | None | RETAIN | Identity user, retry metadata and counts. No customer link. [Migration 0096](../../backend/migrations/0096_login_lockout_notifications.sql#L2) |
| `message_attachments` | conversations | provider_media_id, storage_key, file_name, object bytes | message_id | ERASE | Record object deletion progress before removing the row. Fence in-flight fetches. [Migration 0057](../../backend/migrations/0057_whatsapp_channel.sql#L72) |
| `message_citations` | conversations | item_title, passage_text | message_id | ERASE | Remove the copy of the retrieved passage. Review the source separately if itself personal. [Migration 0047](../../backend/migrations/0047_embeddings_rag.sql#L61) |
| `messages` | conversations | body | conversation_id | ANONYMIZE | Scrub all kinds, including agents, AI, notes and logged messages. Preserve sender kind and sequencing. [Migration 0034](../../backend/migrations/0034_messages.sql#L34) |
| `mfa_factors` | identity | None attributed | None | RETAIN | Encrypted identity-account factor. No customer link. [Migration 0077](../../backend/migrations/0077_mfa.sql#L9) |
| `mfa_recovery_codes` | identity | None attributed | None | RETAIN | Hashed identity recovery codes. No customer link. [Migration 0077](../../backend/migrations/0077_mfa.sql#L31) |
| `notifications` | notifications | title, body, dedupe_key if derived from personal values | Typed subject_id plus conversation, escalation or tool references | ANONYMIZE | Preserve recipient state. Resolve subject types before scrubbing and fence deferred notification events. [Migration 0054](../../backend/migrations/0054_notifications.sql#L3) |
| `outbox_events` | Shared outbox | payload, last_error, aggregate_id if externally supplied | Event-type-specific aggregate or payload keys and tenant | ANONYMIZE | Scrub attributed payloads and retire events from delivery, including processed and dead-letter records. Keep unrelated tenant events. [Migration 0002](../../backend/migrations/0002_outbox.sql#L1) |
| `password_reset_requests` | identity | email, ip | Global sign-in identifier, no customer FK | RETAIN | Identity workflow stores submitted email even without a user. A shared email requires a separate scope decision. [Migration 0081](../../backend/migrations/0081_password_reset_tokens.sql#L5) |
| `password_reset_tokens` | identity | None attributed | None | RETAIN | Identity account, hashed token and IP. No customer link. [Migration 0081](../../backend/migrations/0081_password_reset_tokens.sql#L19) |
| `platform_access_sessions` | tenancy | reason, ip | Tenant and staff user. No customer foreign key | ANONYMIZE | Scrub customer-containing reasons under approved attribution rules. Preserve staff access metadata and separately resolve the mirrored audit values. [Migration 0082](../../backend/migrations/0082_platform_access_sessions.sql#L2) |
| `privacy_request_steps` | privacy | Request link and structured step status | Tenant and request ID | RETAIN | Resumable step records. [Migration 0098](../../backend/migrations/0098_customer_privacy.sql) |
| `privacy_requests` | privacy | Customer ID, actor and export object locator | Tenant and customer ID | ANONYMIZE | Retain request metadata and clear locator on expiry or erasure. [Migration 0098](../../backend/migrations/0098_customer_privacy.sql) |
| `refresh_tokens` | identity | None attributed | None | RETAIN | Identity session and token hash. [Migration 0079](../../backend/migrations/0079_auth_sessions_refresh_tokens.sql#L40) |
| `secret_keks` | Shared secrets | None attributed | None | RETAIN | Wrapped keys and scope metadata. Preserve unrelated encrypted data. [Migration 0076](../../backend/migrations/0076_envelope_encryption.sql#L18) |
| `skills` | escalations | name if customer data was pasted | Tenant skill only | RETAIN | Routing configuration. Customer-specific escalations can carry copies. [Migration 0035](../../backend/migrations/0035_agent_skills.sql#L9) |
| `tenant_invitations` | tenancy | None attributed | None | RETAIN | Invitation email, token hash and staff onboarding metadata. Separate identity scope. [Migration 0019](../../backend/migrations/0019_tenant_invitations.sql#L3) |
| `tenant_memberships` | tenancy | None attributed | None | RETAIN | Staff membership and access status. [Migration 0005](../../backend/migrations/0005_tenant_memberships.sql#L1) |
| `tenant_rate_limit_overrides` | tenancy | None attributed | None | RETAIN | Tenant quota settings and scope codes. [Migration 0061](../../backend/migrations/0061_tenant_rate_limit_overrides.sql#L2) |
| `tenant_subscriptions` | billing | None attributed | None | RETAIN | Tenant plan assignments and status. [Migration 0059](../../backend/migrations/0059_billing_foundation.sql#L22) |
| `tenant_tool_policies` | tools | None attributed | None | RETAIN | Tool names and policy flags. Runtime customer values are separate. [Migration 0049](../../backend/migrations/0049_ai_tool_calling.sql#L29) |
| `tenant_tools` | tools | name, description, input_schema, endpoint_url if customer data was pasted | Tenant and tool definition | RETAIN | Configuration. Runtime customer values go into tool requests. Review confirmed copies. [Migration 0049](../../backend/migrations/0049_ai_tool_calling.sql#L6) |
| `tenants` | tenancy | name, slug and tenant contact fields | Tenant only | RETAIN | Workspace and contact data. No customer association. [Migration 0004](../../backend/migrations/0004_tenants.sql#L1) |
| `tool_requests` | tools | arguments, result, error | conversation_id and generation_id | ANONYMIZE | Keep routing IDs for late notification fencing. Clear arguments, results and errors, including late results. [Migration 0049](../../backend/migrations/0049_ai_tool_calling.sql#L44) |
| `users` | identity | Email, display name and credentials of identity account | User, separate identity from customer entity | RETAIN | Do not merge identities by shared email. Global identity is separately scoped. [Migration 0003](../../backend/migrations/0003_users.sql#L1) |
| `whatsapp_message_meta` | whatsapp | wamid, failure_reason | message_id and conversation_id | ERASE | Also fences late provider status updates. Provider copies are external. [Migration 0057](../../backend/migrations/0057_whatsapp_channel.sql#L19) |
| `widget_instances` | widgets | name, display_name, welcome_message if customer data was pasted | Tenant widget only | RETAIN | Shared configuration. Review confirmed copies by provenance. [Migration 0050](../../backend/migrations/0050_website_chat_widget.sql#L6) |
| `widget_sessions` | widgets | token_hash | customer_id | ERASE | The final schema has a token hash but no fingerprint column. Independently invalidate active sessions and clear associated rate-limit keys. [Migration 0050](../../backend/migrations/0050_website_chat_widget.sql#L45) |

## Objects and cache

| Prefix or key family | Owner | Customer data | Subject linkage | Action | Notes |
| --- | --- | --- | --- | --- | --- |
| `whatsapp-media/{tenant_id}/{attachment_id}` | Conversations and WhatsApp | Original bytes and embedded personal metadata | Attachment to message and conversation | ERASE | Delete from storage before deleting the attachment locator. |
| `{tenant_id}/knowledge/{item_id}` | Knowledge | Uploaded knowledge documents may contain copied customer data | Knowledge item only | RETAIN | Confirmed copies require targeted source removal and reindexing. |
| `privacy-exports/{tenant_id}/{request_id}.zip` | Privacy | Export bundle, including files | Request to customer | ERASE | Remove older exports on erasure and sweep expired bundles. |
| `$TMPDIR/customer-export-{uuid}.zip` | Privacy | In-progress export bundle and file bytes | Worker processing a tenant request | ERASE | Private worker staging file, removed when the job exits normally. Host temporary-file cleanup handles orphaned files after abrupt termination. |
| `objects/{snapshot}/{original_key}` | Backup operations | Mirror of application object bytes | Snapshot and original key | RETAIN | Backup lifecycle. Replay erasure after restore. |
| `db/backup-{snapshot}.dump.age` | Backup operations | Encrypted database dump containing table data | Snapshot and restored records | RETAIN | Backup lifecycle. Replay erasure after restore. |
| `rl:widget_message:{identifier}:{epoch}` | Cache, widgets and feedback | Session identifier and rate counter | Widget session UUID | ERASE | Retire only matching session keys. Counters expire automatically. |
| `rl:widget_session:{identifier}:{epoch}` | Cache and server | Client identifier including IP | Global client IP, no customer binding | RETAIN | Short-lived security counters. Shared IPs cannot establish the subject. |
| `rl:webhook:{identifier}:{epoch}` | Cache, integrations and WhatsApp | Connection UUID and coarse failure reason | Connection only | RETAIN | Short-lived security counters. |
| `rl:tenant_global:{identifier}:{epoch}` | Cache | Tenant UUID and count | Tenant only | RETAIN | Short-lived quota counter. |
| `rl:login:{identifier}:{epoch}` | Cache and identity | Submitted login email, IP and count | Global sign-in address, no customer binding | RETAIN | Identity security TTL. Shared email alone cannot authorize deletion. |
| `rl:password_forgot_email:{identifier}:{epoch}` | Cache and identity | Keyed email fingerprint and count | Global sign-in address | RETAIN | Identity security TTL. |
| `rl:password_forgot_ip:{identifier}:{epoch}` | Cache and identity | Client IP and count | Global client | RETAIN | Identity security TTL. |
| `rl:password_reset_ip:{identifier}:{epoch}` | Cache and identity | Client IP and count | Global client | RETAIN | Identity security TTL. |
| `rl:mfa:{identifier}:{epoch}` | Cache and identity | Client IP and count | Global client | RETAIN | Identity security TTL. |
| `rl:mfa_enrol:{identifier}:{epoch}` | Cache and identity | User UUID and count | Identity account | RETAIN | Identity security TTL. |
| `rl:refresh:{identifier}:{epoch}` | Cache and identity | Client IP and count | Global client | RETAIN | Identity security TTL. |
| `rl:client_errors:{identifier}:{epoch}` | Cache and server | Client tracking identifier and count | User or IP, no customer binding | RETAIN | Diagnostics throttle. |
| `rl:client_errors_counted:{identifier}:{epoch}` | Cache and server | Client tracking identifier and count | User or IP, no customer binding | RETAIN | Diagnostics throttle. |
| `customer-service:auth:stream-revalidation:v1` | Cache | Session, user or membership IDs | Identity scope | RETAIN | Transient Redis pub/sub, with no durable key or customer content. |

## Coverage decisions

- Free text can name more than one person. Scrub all attributed conversation copies, including replies,
  notes, AI context, tool results and copied labels. Tenant-wide knowledge, global identity data
  and payloads without attribution need a reviewed policy. A name or IP match alone does not prove
  identity.
- Append-only audit includes user-supplied reasons. Historical audit rows are retained as security and accountability records. Unattributed
  immutable prompt versions are retained.
- An erasure cannot change existing backup media. Keep a durable erasure ledger outside the restore
  point and replay it before restoring customer access. Object versioning, provider copies and telemetry
  require separate retention and removal procedures.
