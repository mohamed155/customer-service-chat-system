-- Migration 0073: Row-Level Security on the knowledge, AI and tools tables (MOH-48, step 4b of
-- MOH-31; ADR-004 sections 2 and 6). Same rule and policy as 0072.
--
-- pgvector's `knowledge_chunks` is included: ADR-004 section 6 says RLS applies to the vector
-- table too, so a similarity query without a tenant matches nothing.
--
-- `ai_configurations` and `ai_credentials` also hold the platform default rows (tenant_id IS
-- NULL). The strict policy hides them from every tenant query, on purpose: a tenant resolves the
-- platform default only through the audited functions `xtenant.platform_ai_config`,
-- `platform_ai_credential_secret` and `platform_ai_credential_hint` (0071), and the platform routes
-- write them through `platform_ai_config_set` and the credential functions. So a bug in
-- tenant-scoped code cannot read or change the platform's API keys.

DO $$
DECLARE
  t text;
BEGIN
  FOREACH t IN ARRAY ARRAY[
    'knowledge_categories',
    'knowledge_items',
    'knowledge_documents',
    'knowledge_item_tags',
    'knowledge_chunks',
    'knowledge_index_state',
    'ai_configurations',
    'ai_credentials',
    'ai_usage_records',
    'ai_generations',
    'agent_configurations',
    'agent_prompts',
    'agent_prompt_versions',
    'agent_avatar_uploads',
    'tenant_tools',
    'tenant_tool_policies',
    'tool_requests'
  ] LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
    EXECUTE format(
      'CREATE POLICY tenant_isolation ON %I '
      'USING (tenant_id = NULLIF(current_setting(''app.tenant_id'', true), '''')::uuid) '
      'WITH CHECK (tenant_id = NULLIF(current_setting(''app.tenant_id'', true), '''')::uuid)',
      t);
  END LOOP;
END
$$;
