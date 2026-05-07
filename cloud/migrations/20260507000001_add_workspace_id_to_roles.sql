-- Migration: Add workspace_id to roles table for multi-tenant isolation
-- Created: 2026-05-07

ALTER TABLE roles ADD COLUMN workspace_id TEXT;

CREATE INDEX IF NOT EXISTS idx_roles_workspace ON roles(workspace_id);
