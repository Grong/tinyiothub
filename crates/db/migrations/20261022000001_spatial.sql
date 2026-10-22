-- E4d：空间模型持久化（maps / places / things 坐标列 / workspaces.zone_source）。
-- additive：只新建表与增列，不改写基线；maps 为 zone 权威（E4 裁定 #4）。

CREATE TABLE maps (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    bounds_json TEXT NOT NULL,
    anchor_map_id TEXT REFERENCES maps(id) ON DELETE SET NULL,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(workspace_id, name)
);
CREATE INDEX idx_maps_workspace ON maps(workspace_id);
CREATE INDEX idx_maps_default ON maps(workspace_id, is_default);

CREATE TABLE places (
    id TEXT PRIMARY KEY,
    map_id TEXT NOT NULL REFERENCES maps(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    geometry_json TEXT NOT NULL,
    heading REAL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(map_id, name)
);
CREATE INDEX idx_places_map ON places(map_id);

ALTER TABLE things ADD COLUMN map_id TEXT REFERENCES maps(id) ON DELETE SET NULL;
ALTER TABLE things ADD COLUMN position_x REAL;
ALTER TABLE things ADD COLUMN position_y REAL;
ALTER TABLE things ADD COLUMN position_level INTEGER;
ALTER TABLE things ADD COLUMN position_heading REAL;
ALTER TABLE things ADD COLUMN position_trust TEXT;
ALTER TABLE things ADD COLUMN position_updated_at TEXT;

ALTER TABLE workspaces ADD COLUMN zone_source TEXT NOT NULL DEFAULT 'hierarchy'
    CHECK (zone_source IN ('hierarchy', 'maps'));
