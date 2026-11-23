-- MOH-98 expand: make references between tenant-owned tables include tenant_id.
-- SQLx wraps each migration file in one transaction. Keep expansion (0092), validation (0093),
-- and contraction (0094) separate so ALTER TABLE locks do not span validation scans.

-- A failed CREATE INDEX CONCURRENTLY can leave an invalid index that IF NOT EXISTS will skip.
-- Check the nine referenced parent indexes first and name every missing or invalid index clearly.
DO $$
DECLARE
    index_ref record;
    invalid_indexes text[] := ARRAY[]::text[];
BEGIN
    FOR index_ref IN
        SELECT refs.table_name, refs.index_name, index_data.indisvalid
        FROM (VALUES
            ('agent_configurations', 'agent_configurations_tenant_id_id_uq'),
            ('agent_prompts', 'agent_prompts_tenant_id_id_uq'),
            ('ai_usage_records', 'ai_usage_records_tenant_id_id_uq'),
            ('widget_sessions', 'widget_sessions_tenant_id_id_uq'),
            ('widget_instances', 'widget_instances_tenant_id_id_uq'),
            ('integration_connections', 'integration_connections_tenant_id_id_uq'),
            ('knowledge_categories', 'knowledge_categories_tenant_id_id_uq'),
            ('tenant_tools', 'tenant_tools_tenant_id_id_uq'),
            ('ai_generations', 'ai_generations_tenant_id_id_uq')
        ) AS refs(table_name, index_name)
        LEFT JOIN pg_class table_data
            ON table_data.relnamespace = 'public'::regnamespace
            AND table_data.relname = refs.table_name
        LEFT JOIN pg_class index_relation
            ON index_relation.relnamespace = 'public'::regnamespace
            AND index_relation.relname = refs.index_name
        LEFT JOIN pg_index index_data
            ON index_data.indexrelid = index_relation.oid
            AND index_data.indrelid = table_data.oid
    LOOP
        IF index_ref.indisvalid IS NULL THEN
            invalid_indexes := array_append(
                invalid_indexes,
                format('%s (missing on %s)', index_ref.index_name, index_ref.table_name)
            );
        ELSIF NOT index_ref.indisvalid THEN
            invalid_indexes := array_append(
                invalid_indexes,
                format('%s (invalid)', index_ref.index_name)
            );
        END IF;
    END LOOP;

    IF cardinality(invalid_indexes) > 0 THEN
        RAISE EXCEPTION 'MOH-98 parent-index preflight failed: %',
            array_to_string(invalid_indexes, '; ')
            USING HINT = 'Run REINDEX INDEX CONCURRENTLY <name> for an existing invalid index, or drop and rerun the corresponding index migration if missing.';
    END IF;
END
$$;

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

-- Expand: the new constraints are enforced for new writes immediately, while
-- validation in 0093 checks every pre-existing row before any old FK is removed in 0094.
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
    ADD CONSTRAINT knowledge_chunks_tenant_id_item_id_cascade_fkey
    FOREIGN KEY (tenant_id, item_id) REFERENCES knowledge_items (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE knowledge_documents
    ADD CONSTRAINT knowledge_documents_tenant_id_item_id_fkey
    FOREIGN KEY (tenant_id, item_id) REFERENCES knowledge_items (tenant_id, id)
    MATCH SIMPLE ON UPDATE NO ACTION ON DELETE CASCADE NOT VALID;
ALTER TABLE knowledge_index_state
    ADD CONSTRAINT knowledge_index_state_tenant_id_item_id_cascade_fkey
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
