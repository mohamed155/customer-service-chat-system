-- MOH-98: make references between tenant-owned tables include tenant_id.

-- app_owner is subject to FORCE ROW LEVEL SECURITY. This platform role has
-- SELECT on tenant tables and BYPASSRLS, so the preflight sees every tenant.
SET LOCAL ROLE app_platform;
DO $$
DECLARE
    ref record;
    violation_count bigint;
    violations text[] := ARRAY[]::text[];
BEGIN
    FOR ref IN
        SELECT *
        FROM (VALUES
            ('agent_avatar_uploads', 'agent_id', 'agent_configurations'),
            ('agent_prompt_versions', 'prompt_id', 'agent_prompts'),
            ('ai_generations', 'conversation_id', 'conversations'),
            ('ai_generations', 'response_message_id', 'messages'),
            ('ai_generations', 'trigger_message_id', 'messages'),
            ('ai_generations', 'usage_record_id', 'ai_usage_records'),
            ('conversation_feedback', 'agent_configuration_id', 'agent_configurations'),
            ('conversation_feedback', 'widget_session_id', 'widget_sessions'),
            ('conversations', 'widget_instance_id', 'widget_instances'),
            ('integration_connections', 'connected_by_membership_id', 'tenant_memberships'),
            ('integration_connections', 'disconnected_by_membership_id', 'tenant_memberships'),
            ('integration_events', 'actor_membership_id', 'tenant_memberships'),
            ('integration_events', 'connection_id', 'integration_connections'),
            ('integration_secrets', 'connection_id', 'integration_connections'),
            ('integration_webhook_deliveries', 'connection_id', 'integration_connections'),
            ('knowledge_chunks', 'item_id', 'knowledge_items'),
            ('knowledge_documents', 'item_id', 'knowledge_items'),
            ('knowledge_index_state', 'item_id', 'knowledge_items'),
            ('knowledge_item_tags', 'item_id', 'knowledge_items'),
            ('knowledge_items', 'category_id', 'knowledge_categories'),
            ('message_citations', 'message_id', 'messages'),
            ('notifications', 'actor_membership_id', 'tenant_memberships'),
            ('notifications', 'recipient_membership_id', 'tenant_memberships'),
            ('tenant_tool_policies', 'updated_by_membership_id', 'tenant_memberships'),
            ('tenant_tools', 'created_by_membership_id', 'tenant_memberships'),
            ('tool_requests', 'conversation_id', 'conversations'),
            ('tool_requests', 'decided_by_membership_id', 'tenant_memberships'),
            ('tool_requests', 'generation_id', 'ai_generations'),
            ('tool_requests', 'tenant_tool_id', 'tenant_tools'),
            ('widget_sessions', 'customer_id', 'customers'),
            ('widget_sessions', 'widget_instance_id', 'widget_instances')
        ) AS refs(child_table, child_column, parent_table)
    LOOP
        EXECUTE format(
            'SELECT count(*) FROM %I AS child ' ||
            'LEFT JOIN %I AS parent ON parent.id = child.%I ' ||
            'WHERE child.%I IS NOT NULL ' ||
            'AND (parent.id IS NULL OR parent.tenant_id IS DISTINCT FROM child.tenant_id)',
            ref.child_table,
            ref.parent_table,
            ref.child_column,
            ref.child_column
        ) INTO violation_count;

        IF violation_count > 0 THEN
            violations := array_append(
                violations,
                format('%s.%s: %s row(s)', ref.child_table, ref.child_column, violation_count)
            );
        END IF;
    END LOOP;

    IF cardinality(violations) > 0 THEN
        RAISE EXCEPTION 'MOH-98 foreign-key preflight found cross-tenant references: %',
            array_to_string(violations, '; ');
    END IF;
END
$$;
RESET ROLE;

-- Keep the old NO ACTION constraints active until their CASCADE replacements
-- have been validated. Their names conflict with the required final names.
ALTER TABLE knowledge_chunks
    RENAME CONSTRAINT knowledge_chunks_tenant_id_item_id_fkey
    TO knowledge_chunks_tenant_id_item_id_legacy_no_action_fkey;
ALTER TABLE knowledge_index_state
    RENAME CONSTRAINT knowledge_index_state_tenant_id_item_id_fkey
    TO knowledge_index_state_tenant_id_item_id_legacy_no_action_fkey;

-- Expand: the new constraints are enforced for new writes immediately, while
-- validation checks every pre-existing row before any old FK is removed.
ALTER TABLE agent_avatar_uploads
    ADD CONSTRAINT agent_avatar_uploads_tenant_id_agent_id_fkey
    FOREIGN KEY (tenant_id, agent_id) REFERENCES agent_configurations (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE agent_prompt_versions
    ADD CONSTRAINT agent_prompt_versions_tenant_id_prompt_id_fkey
    FOREIGN KEY (tenant_id, prompt_id) REFERENCES agent_prompts (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE RESTRICT NOT VALID;
ALTER TABLE ai_generations
    ADD CONSTRAINT ai_generations_tenant_id_conversation_id_fkey
    FOREIGN KEY (tenant_id, conversation_id) REFERENCES conversations (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE RESTRICT NOT VALID;
ALTER TABLE ai_generations
    ADD CONSTRAINT ai_generations_tenant_id_response_message_id_fkey
    FOREIGN KEY (tenant_id, response_message_id) REFERENCES messages (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE SET NULL (response_message_id) NOT VALID;
ALTER TABLE ai_generations
    ADD CONSTRAINT ai_generations_tenant_id_trigger_message_id_fkey
    FOREIGN KEY (tenant_id, trigger_message_id) REFERENCES messages (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE RESTRICT NOT VALID;
ALTER TABLE ai_generations
    ADD CONSTRAINT ai_generations_tenant_id_usage_record_id_fkey
    FOREIGN KEY (tenant_id, usage_record_id) REFERENCES ai_usage_records (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE SET NULL (usage_record_id) NOT VALID;
ALTER TABLE conversation_feedback
    ADD CONSTRAINT conversation_feedback_tenant_id_agent_configuration_id_fkey
    FOREIGN KEY (tenant_id, agent_configuration_id) REFERENCES agent_configurations (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE RESTRICT NOT VALID;
ALTER TABLE conversation_feedback
    ADD CONSTRAINT conversation_feedback_tenant_id_widget_session_id_fkey
    FOREIGN KEY (tenant_id, widget_session_id) REFERENCES widget_sessions (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE SET NULL (widget_session_id) NOT VALID;
ALTER TABLE conversations
    ADD CONSTRAINT conversations_tenant_id_widget_instance_id_fkey
    FOREIGN KEY (tenant_id, widget_instance_id) REFERENCES widget_instances (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE SET NULL (widget_instance_id) NOT VALID;
ALTER TABLE integration_connections
    ADD CONSTRAINT integration_connections_tenant_id_connected_by_membership_id_fkey
    FOREIGN KEY (tenant_id, connected_by_membership_id) REFERENCES tenant_memberships (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE NO ACTION NOT VALID;
ALTER TABLE integration_connections
    ADD CONSTRAINT integration_connections_tenant_id_disconnected_by_membership_id_fkey
    FOREIGN KEY (tenant_id, disconnected_by_membership_id) REFERENCES tenant_memberships (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE NO ACTION NOT VALID;
ALTER TABLE integration_events
    ADD CONSTRAINT integration_events_tenant_id_actor_membership_id_fkey
    FOREIGN KEY (tenant_id, actor_membership_id) REFERENCES tenant_memberships (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE NO ACTION NOT VALID;
ALTER TABLE integration_events
    ADD CONSTRAINT integration_events_tenant_id_connection_id_fkey
    FOREIGN KEY (tenant_id, connection_id) REFERENCES integration_connections (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE integration_secrets
    ADD CONSTRAINT integration_secrets_tenant_id_connection_id_fkey
    FOREIGN KEY (tenant_id, connection_id) REFERENCES integration_connections (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE integration_webhook_deliveries
    ADD CONSTRAINT integration_webhook_deliveries_tenant_id_connection_id_fkey
    FOREIGN KEY (tenant_id, connection_id) REFERENCES integration_connections (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE knowledge_chunks
    ADD CONSTRAINT knowledge_chunks_tenant_id_item_id_fkey
    FOREIGN KEY (tenant_id, item_id) REFERENCES knowledge_items (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE knowledge_documents
    ADD CONSTRAINT knowledge_documents_tenant_id_item_id_fkey
    FOREIGN KEY (tenant_id, item_id) REFERENCES knowledge_items (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE knowledge_index_state
    ADD CONSTRAINT knowledge_index_state_tenant_id_item_id_fkey
    FOREIGN KEY (tenant_id, item_id) REFERENCES knowledge_items (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE knowledge_item_tags
    ADD CONSTRAINT knowledge_item_tags_tenant_id_item_id_fkey
    FOREIGN KEY (tenant_id, item_id) REFERENCES knowledge_items (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE knowledge_items
    ADD CONSTRAINT knowledge_items_tenant_id_category_id_fkey
    FOREIGN KEY (tenant_id, category_id) REFERENCES knowledge_categories (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE SET NULL (category_id) NOT VALID;
ALTER TABLE message_citations
    ADD CONSTRAINT message_citations_tenant_id_message_id_fkey
    FOREIGN KEY (tenant_id, message_id) REFERENCES messages (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE notifications
    ADD CONSTRAINT notifications_tenant_id_actor_membership_id_fkey
    FOREIGN KEY (tenant_id, actor_membership_id) REFERENCES tenant_memberships (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE NO ACTION NOT VALID;
ALTER TABLE notifications
    ADD CONSTRAINT notifications_tenant_id_recipient_membership_id_fkey
    FOREIGN KEY (tenant_id, recipient_membership_id) REFERENCES tenant_memberships (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE tenant_tool_policies
    ADD CONSTRAINT tenant_tool_policies_tenant_id_updated_by_membership_id_fkey
    FOREIGN KEY (tenant_id, updated_by_membership_id) REFERENCES tenant_memberships (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE NO ACTION NOT VALID;
ALTER TABLE tenant_tools
    ADD CONSTRAINT tenant_tools_tenant_id_created_by_membership_id_fkey
    FOREIGN KEY (tenant_id, created_by_membership_id) REFERENCES tenant_memberships (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE NO ACTION NOT VALID;
ALTER TABLE tool_requests
    ADD CONSTRAINT tool_requests_tenant_id_conversation_id_fkey
    FOREIGN KEY (tenant_id, conversation_id) REFERENCES conversations (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE NO ACTION NOT VALID;
ALTER TABLE tool_requests
    ADD CONSTRAINT tool_requests_tenant_id_decided_by_membership_id_fkey
    FOREIGN KEY (tenant_id, decided_by_membership_id) REFERENCES tenant_memberships (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE NO ACTION NOT VALID;
ALTER TABLE tool_requests
    ADD CONSTRAINT tool_requests_tenant_id_generation_id_fkey
    FOREIGN KEY (tenant_id, generation_id) REFERENCES ai_generations (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE NO ACTION NOT VALID;
ALTER TABLE tool_requests
    ADD CONSTRAINT tool_requests_tenant_id_tenant_tool_id_fkey
    FOREIGN KEY (tenant_id, tenant_tool_id) REFERENCES tenant_tools (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE NO ACTION NOT VALID;
ALTER TABLE widget_sessions
    ADD CONSTRAINT widget_sessions_tenant_id_customer_id_fkey
    FOREIGN KEY (tenant_id, customer_id) REFERENCES customers (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE SET NULL (customer_id) NOT VALID;
ALTER TABLE widget_sessions
    ADD CONSTRAINT widget_sessions_tenant_id_widget_instance_id_fkey
    FOREIGN KEY (tenant_id, widget_instance_id) REFERENCES widget_instances (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE RESTRICT NOT VALID;

-- Validate the expanded references before removing any original constraints.
ALTER TABLE agent_avatar_uploads VALIDATE CONSTRAINT agent_avatar_uploads_tenant_id_agent_id_fkey;
ALTER TABLE agent_prompt_versions VALIDATE CONSTRAINT agent_prompt_versions_tenant_id_prompt_id_fkey;
ALTER TABLE ai_generations VALIDATE CONSTRAINT ai_generations_tenant_id_conversation_id_fkey;
ALTER TABLE ai_generations VALIDATE CONSTRAINT ai_generations_tenant_id_response_message_id_fkey;
ALTER TABLE ai_generations VALIDATE CONSTRAINT ai_generations_tenant_id_trigger_message_id_fkey;
ALTER TABLE ai_generations VALIDATE CONSTRAINT ai_generations_tenant_id_usage_record_id_fkey;
ALTER TABLE conversation_feedback VALIDATE CONSTRAINT conversation_feedback_tenant_id_agent_configuration_id_fkey;
ALTER TABLE conversation_feedback VALIDATE CONSTRAINT conversation_feedback_tenant_id_widget_session_id_fkey;
ALTER TABLE conversations VALIDATE CONSTRAINT conversations_tenant_id_widget_instance_id_fkey;
ALTER TABLE integration_connections VALIDATE CONSTRAINT integration_connections_tenant_id_connected_by_membership_id_fkey;
ALTER TABLE integration_connections VALIDATE CONSTRAINT integration_connections_tenant_id_disconnected_by_membership_id_fkey;
ALTER TABLE integration_events VALIDATE CONSTRAINT integration_events_tenant_id_actor_membership_id_fkey;
ALTER TABLE integration_events VALIDATE CONSTRAINT integration_events_tenant_id_connection_id_fkey;
ALTER TABLE integration_secrets VALIDATE CONSTRAINT integration_secrets_tenant_id_connection_id_fkey;
ALTER TABLE integration_webhook_deliveries VALIDATE CONSTRAINT integration_webhook_deliveries_tenant_id_connection_id_fkey;
ALTER TABLE knowledge_chunks VALIDATE CONSTRAINT knowledge_chunks_tenant_id_item_id_fkey;
ALTER TABLE knowledge_documents VALIDATE CONSTRAINT knowledge_documents_tenant_id_item_id_fkey;
ALTER TABLE knowledge_index_state VALIDATE CONSTRAINT knowledge_index_state_tenant_id_item_id_fkey;
ALTER TABLE knowledge_item_tags VALIDATE CONSTRAINT knowledge_item_tags_tenant_id_item_id_fkey;
ALTER TABLE knowledge_items VALIDATE CONSTRAINT knowledge_items_tenant_id_category_id_fkey;
ALTER TABLE message_citations VALIDATE CONSTRAINT message_citations_tenant_id_message_id_fkey;
ALTER TABLE notifications VALIDATE CONSTRAINT notifications_tenant_id_actor_membership_id_fkey;
ALTER TABLE notifications VALIDATE CONSTRAINT notifications_tenant_id_recipient_membership_id_fkey;
ALTER TABLE tenant_tool_policies VALIDATE CONSTRAINT tenant_tool_policies_tenant_id_updated_by_membership_id_fkey;
ALTER TABLE tenant_tools VALIDATE CONSTRAINT tenant_tools_tenant_id_created_by_membership_id_fkey;
ALTER TABLE tool_requests VALIDATE CONSTRAINT tool_requests_tenant_id_conversation_id_fkey;
ALTER TABLE tool_requests VALIDATE CONSTRAINT tool_requests_tenant_id_decided_by_membership_id_fkey;
ALTER TABLE tool_requests VALIDATE CONSTRAINT tool_requests_tenant_id_generation_id_fkey;
ALTER TABLE tool_requests VALIDATE CONSTRAINT tool_requests_tenant_id_tenant_tool_id_fkey;
ALTER TABLE widget_sessions VALIDATE CONSTRAINT widget_sessions_tenant_id_customer_id_fkey;
ALTER TABLE widget_sessions VALIDATE CONSTRAINT widget_sessions_tenant_id_widget_instance_id_fkey;

-- Contract: the composite constraints are now validated, so the old single-key
-- constraints and the superseded NO ACTION duplicates are no longer needed.
ALTER TABLE agent_avatar_uploads DROP CONSTRAINT agent_avatar_uploads_agent_id_fkey;
ALTER TABLE agent_prompt_versions DROP CONSTRAINT agent_prompt_versions_prompt_id_fkey;
ALTER TABLE ai_generations DROP CONSTRAINT ai_generations_conversation_id_fkey;
ALTER TABLE ai_generations DROP CONSTRAINT ai_generations_response_message_id_fkey;
ALTER TABLE ai_generations DROP CONSTRAINT ai_generations_trigger_message_id_fkey;
ALTER TABLE ai_generations DROP CONSTRAINT ai_generations_usage_record_id_fkey;
ALTER TABLE conversation_feedback DROP CONSTRAINT conversation_feedback_agent_configuration_id_fkey;
ALTER TABLE conversation_feedback DROP CONSTRAINT conversation_feedback_widget_session_id_fkey;
ALTER TABLE conversations DROP CONSTRAINT conversations_widget_instance_id_fkey;
ALTER TABLE integration_connections DROP CONSTRAINT integration_connections_connected_by_membership_id_fkey;
ALTER TABLE integration_connections DROP CONSTRAINT integration_connections_disconnected_by_membership_id_fkey;
ALTER TABLE integration_events DROP CONSTRAINT integration_events_actor_membership_id_fkey;
ALTER TABLE integration_events DROP CONSTRAINT integration_events_connection_id_fkey;
ALTER TABLE integration_secrets DROP CONSTRAINT integration_secrets_connection_id_fkey;
ALTER TABLE integration_webhook_deliveries DROP CONSTRAINT integration_webhook_deliveries_connection_id_fkey;
ALTER TABLE knowledge_chunks DROP CONSTRAINT knowledge_chunks_item_id_fkey;
ALTER TABLE knowledge_documents DROP CONSTRAINT knowledge_documents_item_id_fkey;
ALTER TABLE knowledge_index_state DROP CONSTRAINT knowledge_index_state_item_id_fkey;
ALTER TABLE knowledge_item_tags DROP CONSTRAINT knowledge_item_tags_item_id_fkey;
ALTER TABLE knowledge_items DROP CONSTRAINT knowledge_items_category_id_fkey;
ALTER TABLE message_citations DROP CONSTRAINT message_citations_message_id_fkey;
ALTER TABLE notifications DROP CONSTRAINT notifications_actor_membership_id_fkey;
ALTER TABLE notifications DROP CONSTRAINT notifications_recipient_membership_id_fkey;
ALTER TABLE tenant_tool_policies DROP CONSTRAINT tenant_tool_policies_updated_by_membership_id_fkey;
ALTER TABLE tenant_tools DROP CONSTRAINT tenant_tools_created_by_membership_id_fkey;
ALTER TABLE tool_requests DROP CONSTRAINT tool_requests_conversation_id_fkey;
ALTER TABLE tool_requests DROP CONSTRAINT tool_requests_decided_by_membership_id_fkey;
ALTER TABLE tool_requests DROP CONSTRAINT tool_requests_generation_id_fkey;
ALTER TABLE tool_requests DROP CONSTRAINT tool_requests_tenant_tool_id_fkey;
ALTER TABLE widget_sessions DROP CONSTRAINT widget_sessions_customer_id_fkey;
ALTER TABLE widget_sessions DROP CONSTRAINT widget_sessions_widget_instance_id_fkey;
ALTER TABLE knowledge_chunks DROP CONSTRAINT knowledge_chunks_tenant_id_item_id_legacy_no_action_fkey;
ALTER TABLE knowledge_index_state DROP CONSTRAINT knowledge_index_state_tenant_id_item_id_legacy_no_action_fkey;
