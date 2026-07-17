-- Migration 0046: Knowledge base — categories, items, documents, and tags
-- for tenant-scoped knowledge management. See specs/019-knowledge-base/
-- for the full design.

-- Table: knowledge_categories — flat per-tenant category list (hard-delete).
CREATE TABLE knowledge_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX knowledge_categories_tenant_name_uq
    ON knowledge_categories (tenant_id, lower(name));

CREATE OR REPLACE FUNCTION set_knowledge_categories_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_knowledge_categories_updated_at
    BEFORE UPDATE ON knowledge_categories
    FOR EACH ROW
    EXECUTE FUNCTION set_knowledge_categories_updated_at();

-- Table: knowledge_items — one row per knowledge item of any type.
CREATE TABLE knowledge_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    item_type TEXT NOT NULL CHECK (item_type IN ('article', 'faq', 'document')),
    title TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200),
    body TEXT NULL CHECK (char_length(body) <= 100000),
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    category_id UUID NULL REFERENCES knowledge_categories(id) ON DELETE SET NULL,
    source TEXT NOT NULL CHECK (source IN ('authored', 'uploaded')),
    created_by_user_id UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    created_by_display TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT knowledge_items_document_no_body CHECK (item_type <> 'document' OR body IS NULL)
);

CREATE INDEX knowledge_items_tenant_updated_idx
    ON knowledge_items (tenant_id, updated_at DESC, id DESC);

CREATE INDEX knowledge_items_tenant_status_idx
    ON knowledge_items (tenant_id, status);

CREATE INDEX knowledge_items_category_idx
    ON knowledge_items (category_id);

CREATE OR REPLACE FUNCTION set_knowledge_items_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_knowledge_items_updated_at
    BEFORE UPDATE ON knowledge_items
    FOR EACH ROW
    EXECUTE FUNCTION set_knowledge_items_updated_at();

-- Table: knowledge_documents — 1:1 extension for document items (write-once).
CREATE TABLE knowledge_documents (
    item_id UUID PRIMARY KEY REFERENCES knowledge_items(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    storage_key TEXT NOT NULL UNIQUE,
    original_filename TEXT NOT NULL CHECK (char_length(original_filename) BETWEEN 1 AND 255),
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes > 0 AND size_bytes <= 20971520),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Table: knowledge_item_tags — tag-as-value join rows.
CREATE TABLE knowledge_item_tags (
    item_id UUID NOT NULL REFERENCES knowledge_items(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    tag TEXT NOT NULL CHECK (char_length(tag) BETWEEN 1 AND 40),
    PRIMARY KEY (item_id, tag)
);

CREATE INDEX knowledge_item_tags_tenant_tag_idx
    ON knowledge_item_tags (tenant_id, tag);
