-- Add tenant_id to tags and tag_bindings for multi-tenant isolation

ALTER TABLE tags ADD COLUMN tenant_id TEXT;
ALTER TABLE tag_bindings ADD COLUMN tenant_id TEXT;

-- Backfill existing tags with a default tenant if needed
-- UPDATE tags SET tenant_id = 'default' WHERE tenant_id IS NULL;
-- UPDATE tag_bindings SET tenant_id = 'default' WHERE tenant_id IS NULL;

CREATE INDEX idx_tags_tenant_id ON tags(tenant_id);
CREATE INDEX idx_tag_bindings_tenant_id ON tag_bindings(tenant_id);
