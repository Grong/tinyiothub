//! E4d 空间模型持久化：maps / places CRUD、物坐标列、workspaces.zone_source。
//!
//! 几何/边界以 JSON 文本落库（`bounds_json` / `geometry_json`），serde 只在
//! 本适配层发生；runtime 经 `MapStore` port 使用 core 值类型。所有 SQL 以
//! workspace 为作用域（place 经 maps 关联）。

use sqlx::{FromRow, SqlitePool};
use tinyiothub_core::models::spatial::{
    Geometry, LocationTrust, MapBounds, MapRecord, PlaceKind, PlaceRecord, ThingPosition,
};

use crate::database::Db;

// ── Row 类型与转换 ──────────────────────────────────────

#[derive(Debug, Clone, FromRow)]
struct MapRow {
    id: String,
    workspace_id: String,
    name: String,
    bounds_json: String,
    anchor_map_id: Option<String>,
    is_default: i64,
    created_at: String,
    updated_at: String,
}

impl TryFrom<MapRow> for MapRecord {
    type Error = sqlx::Error;

    fn try_from(row: MapRow) -> Result<Self, Self::Error> {
        let bounds: MapBounds = serde_json::from_str(&row.bounds_json).map_err(decode_err)?;
        Ok(Self {
            id: row.id,
            workspace_id: row.workspace_id,
            name: row.name,
            bounds,
            anchor_map_id: row.anchor_map_id,
            is_default: row.is_default != 0,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, Clone, FromRow)]
struct PlaceRow {
    id: String,
    map_id: String,
    name: String,
    kind: String,
    geometry_json: String,
    heading: Option<f64>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<PlaceRow> for PlaceRecord {
    type Error = sqlx::Error;

    fn try_from(row: PlaceRow) -> Result<Self, Self::Error> {
        let geometry: Geometry = serde_json::from_str(&row.geometry_json).map_err(decode_err)?;
        Ok(Self {
            id: row.id,
            map_id: row.map_id,
            name: row.name,
            kind: PlaceKind::from_db(&row.kind),
            geometry,
            heading: row.heading,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

fn decode_err(e: serde_json::Error) -> sqlx::Error {
    sqlx::Error::Decode(Box::new(e))
}

fn encode_err(e: serde_json::Error) -> sqlx::Error {
    sqlx::Error::Encode(Box::new(e))
}

/// 跨工作区引用一律作错误返回（不是静默 no-op）：防越权读写与重归属。
fn isolation_err(msg: &str) -> sqlx::Error {
    sqlx::Error::Configuration(msg.to_string().into())
}

// ── Maps ───────────────────────────────────────────────

pub async fn list_maps(pool: &SqlitePool, workspace_id: &str) -> Result<Vec<MapRecord>, sqlx::Error> {
    let rows: Vec<MapRow> = sqlx::query_as(
        "SELECT id, workspace_id, name, bounds_json, anchor_map_id, is_default, created_at, updated_at
         FROM maps WHERE workspace_id = ? ORDER BY name ASC",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(MapRecord::try_from).collect()
}

pub async fn find_map(pool: &SqlitePool, workspace_id: &str, id: &str) -> Result<Option<MapRecord>, sqlx::Error> {
    let row: Option<MapRow> = sqlx::query_as(
        "SELECT id, workspace_id, name, bounds_json, anchor_map_id, is_default, created_at, updated_at
         FROM maps WHERE workspace_id = ? AND id = ?",
    )
    .bind(workspace_id)
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.map(MapRecord::try_from).transpose()
}

/// 工作区默认图（应用层保证至多一个 is_default=1）。
pub async fn find_default_map(pool: &SqlitePool, workspace_id: &str) -> Result<Option<MapRecord>, sqlx::Error> {
    let row: Option<MapRow> = sqlx::query_as(
        "SELECT id, workspace_id, name, bounds_json, anchor_map_id, is_default, created_at, updated_at
         FROM maps WHERE workspace_id = ? AND is_default = 1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    row.map(MapRecord::try_from).transpose()
}

/// upsert 地图；`is_default=true` 时在同一事务内清掉同工作区其他默认标记。
/// id 已存在时只允许同工作区更新，禁止跨工作区重归属。
pub async fn upsert_map(pool: &SqlitePool, rec: &MapRecord) -> Result<(), sqlx::Error> {
    let bounds_json = serde_json::to_string(&rec.bounds).map_err(encode_err)?;
    let mut tx = pool.begin().await?;
    let existing_ws: Option<String> = sqlx::query_scalar("SELECT workspace_id FROM maps WHERE id = ?")
        .bind(&rec.id)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some(existing_ws) = existing_ws
        && existing_ws != rec.workspace_id
    {
        return Err(isolation_err("map id belongs to another workspace"));
    }
    if rec.is_default {
        sqlx::query("UPDATE maps SET is_default = 0 WHERE workspace_id = ? AND id <> ?")
            .bind(&rec.workspace_id)
            .bind(&rec.id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "INSERT INTO maps (id, workspace_id, name, bounds_json, anchor_map_id, is_default, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name,
             bounds_json = excluded.bounds_json,
             anchor_map_id = excluded.anchor_map_id,
             is_default = excluded.is_default,
             updated_at = excluded.updated_at",
    )
    .bind(&rec.id)
    .bind(&rec.workspace_id)
    .bind(&rec.name)
    .bind(bounds_json)
    .bind(&rec.anchor_map_id)
    .bind(i64::from(rec.is_default))
    .bind(&rec.created_at)
    .bind(&rec.updated_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete_map(pool: &SqlitePool, workspace_id: &str, id: &str) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("DELETE FROM maps WHERE workspace_id = ? AND id = ?")
        .bind(workspace_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() > 0)
}

// ── Places ─────────────────────────────────────────────

pub async fn list_places(pool: &SqlitePool, workspace_id: &str, map_id: &str) -> Result<Vec<PlaceRecord>, sqlx::Error> {
    let rows: Vec<PlaceRow> = sqlx::query_as(
        "SELECT p.id, p.map_id, p.name, p.kind, p.geometry_json, p.heading, p.created_at, p.updated_at
         FROM places p JOIN maps m ON m.id = p.map_id
         WHERE m.workspace_id = ? AND p.map_id = ?
         ORDER BY p.name ASC",
    )
    .bind(workspace_id)
    .bind(map_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(PlaceRecord::try_from).collect()
}

pub async fn find_place(
    pool: &SqlitePool,
    workspace_id: &str,
    map_id: &str,
    id: &str,
) -> Result<Option<PlaceRecord>, sqlx::Error> {
    let row: Option<PlaceRow> = sqlx::query_as(
        "SELECT p.id, p.map_id, p.name, p.kind, p.geometry_json, p.heading, p.created_at, p.updated_at
         FROM places p JOIN maps m ON m.id = p.map_id
         WHERE m.workspace_id = ? AND p.map_id = ? AND p.id = ?",
    )
    .bind(workspace_id)
    .bind(map_id)
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.map(PlaceRecord::try_from).transpose()
}

/// upsert 地点：目标地图必须属于 `workspace_id`；已存在的同 id 地点也不得
/// 跨工作区重归属。
pub async fn upsert_place(pool: &SqlitePool, workspace_id: &str, rec: &PlaceRecord) -> Result<(), sqlx::Error> {
    let geometry_json = serde_json::to_string(&rec.geometry).map_err(encode_err)?;
    let mut tx = pool.begin().await?;

    let map_owner: Option<String> = sqlx::query_scalar("SELECT workspace_id FROM maps WHERE id = ?")
        .bind(&rec.map_id)
        .fetch_optional(&mut *tx)
        .await?;
    if map_owner.as_deref() != Some(workspace_id) {
        return Err(isolation_err("place map does not belong to workspace"));
    }

    let place_owner: Option<String> =
        sqlx::query_scalar("SELECT m.workspace_id FROM places p JOIN maps m ON m.id = p.map_id WHERE p.id = ?")
            .bind(&rec.id)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(place_owner) = place_owner
        && place_owner != workspace_id
    {
        return Err(isolation_err("place id belongs to another workspace"));
    }

    sqlx::query(
        "INSERT INTO places (id, map_id, name, kind, geometry_json, heading, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
             map_id = excluded.map_id,
             name = excluded.name,
             kind = excluded.kind,
             geometry_json = excluded.geometry_json,
             heading = excluded.heading,
             updated_at = excluded.updated_at",
    )
    .bind(&rec.id)
    .bind(&rec.map_id)
    .bind(&rec.name)
    .bind(rec.kind.as_str())
    .bind(geometry_json)
    .bind(rec.heading)
    .bind(&rec.created_at)
    .bind(&rec.updated_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete_place(pool: &SqlitePool, workspace_id: &str, map_id: &str, id: &str) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "DELETE FROM places
         WHERE id = ? AND map_id = ?
           AND EXISTS (SELECT 1 FROM maps WHERE id = ? AND workspace_id = ?)",
    )
    .bind(id)
    .bind(map_id)
    .bind(map_id)
    .bind(workspace_id)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// 按名称在工作区内找地点：默认图优先、`kind` 可选过滤（zoneRef 解析应传
/// `Some(PlaceKind::Zone)`），`p.id` 作最终确定性 tiebreak。
pub async fn find_place_by_name(
    pool: &SqlitePool,
    workspace_id: &str,
    name: &str,
    kind: Option<PlaceKind>,
) -> Result<Option<PlaceRecord>, sqlx::Error> {
    let kind = kind.map(PlaceKind::as_str);
    let row: Option<PlaceRow> = sqlx::query_as(
        "SELECT p.id, p.map_id, p.name, p.kind, p.geometry_json, p.heading, p.created_at, p.updated_at
         FROM places p JOIN maps m ON m.id = p.map_id
         WHERE m.workspace_id = ? AND p.name = ? AND (? IS NULL OR p.kind = ?)
         ORDER BY m.is_default DESC, p.created_at ASC, p.id ASC
         LIMIT 1",
    )
    .bind(workspace_id)
    .bind(name)
    .bind(kind)
    .bind(kind)
    .fetch_optional(pool)
    .await?;
    row.map(PlaceRecord::try_from).transpose()
}

// ── 物坐标 ─────────────────────────────────────────────

#[derive(Debug, FromRow)]
struct PositionRow {
    id: String,
    map_id: Option<String>,
    position_x: Option<f64>,
    position_y: Option<f64>,
    position_level: Option<i64>,
    position_heading: Option<f64>,
    position_trust: Option<String>,
    position_updated_at: Option<String>,
}

impl PositionRow {
    fn into_position(self) -> ThingPosition {
        ThingPosition {
            map_id: self.map_id,
            x: self.position_x,
            y: self.position_y,
            level: self.position_level,
            heading: self.position_heading,
            trust: LocationTrust::from_db(self.position_trust.as_deref()),
            updated_at: self.position_updated_at,
        }
    }
}

pub async fn get_thing_position(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: &str,
) -> Result<Option<ThingPosition>, sqlx::Error> {
    let row: Option<PositionRow> = sqlx::query_as(
        "SELECT id, map_id, position_x, position_y, position_level, position_heading,
                position_trust, position_updated_at
         FROM things WHERE id = ? AND workspace_id = ?",
    )
    .bind(thing_id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(PositionRow::into_position))
}

/// 工作区全量坐标（快照批量预热：避免逐物 N 次查询）。
pub async fn list_thing_positions(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<(String, ThingPosition)>, sqlx::Error> {
    let rows: Vec<PositionRow> = sqlx::query_as(
        "SELECT id, map_id, position_x, position_y, position_level, position_heading,
                position_trust, position_updated_at
         FROM things WHERE workspace_id = ? ORDER BY id ASC",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let id = row.id.clone();
            (id, row.into_position())
        })
        .collect())
}

/// 写回**驱动读数**坐标列：不改 `map_id`（地图由 placement/API 维护，读数
/// 只报坐标），也不动 `zone_ref`。
pub async fn set_thing_reading_position(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: &str,
    position: &ThingPosition,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE things SET
             position_x = ?, position_y = ?, position_level = ?,
             position_heading = ?, position_trust = ?, position_updated_at = ?
         WHERE id = ? AND workspace_id = ?",
    )
    .bind(position.x)
    .bind(position.y)
    .bind(position.level)
    .bind(position.heading)
    .bind(position.trust.as_str())
    .bind(&position.updated_at)
    .bind(thing_id)
    .bind(workspace_id)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

// ── 显式 zone 引用 ─────────────────────────────────────

pub async fn get_thing_zone_ref(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<Option<String>> =
        sqlx::query_scalar("SELECT zone_ref FROM things WHERE id = ? AND workspace_id = ?")
            .bind(thing_id)
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.flatten())
}

pub async fn set_thing_zone_ref(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: &str,
    zone_ref: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("UPDATE things SET zone_ref = ? WHERE id = ? AND workspace_id = ?")
        .bind(zone_ref)
        .bind(thing_id)
        .bind(workspace_id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() > 0)
}

/// 工作区全部非空显式 zone 引用（快照批量预热）。
pub async fn list_thing_zone_refs(pool: &SqlitePool, workspace_id: &str) -> Result<Vec<(String, String)>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, zone_ref FROM things
         WHERE workspace_id = ? AND zone_ref IS NOT NULL AND zone_ref <> ''
         ORDER BY id ASC",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 写回物坐标列（同一 workspace 才命中）。返回是否找到该物。
/// `position.map_id` 必须属于该 workspace（且存在），否则拒绝。
pub async fn set_thing_position(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: &str,
    position: &ThingPosition,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    if let Some(map_id) = &position.map_id {
        let map_owner: Option<String> = sqlx::query_scalar("SELECT workspace_id FROM maps WHERE id = ?")
            .bind(map_id)
            .fetch_optional(&mut *tx)
            .await?;
        if map_owner.as_deref() != Some(workspace_id) {
            return Err(isolation_err("position map does not belong to workspace"));
        }
    }
    let r = sqlx::query(
        "UPDATE things SET
             map_id = ?, position_x = ?, position_y = ?, position_level = ?,
             position_heading = ?, position_trust = ?, position_updated_at = ?
         WHERE id = ? AND workspace_id = ?",
    )
    .bind(&position.map_id)
    .bind(position.x)
    .bind(position.y)
    .bind(position.level)
    .bind(position.heading)
    .bind(position.trust.as_str())
    .bind(&position.updated_at)
    .bind(thing_id)
    .bind(workspace_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(r.rows_affected() > 0)
}

// ── zone_source ────────────────────────────────────────

pub async fn get_zone_source(pool: &SqlitePool, workspace_id: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT zone_source FROM workspaces WHERE id = ?")
        .bind(workspace_id)
        .fetch_optional(pool)
        .await
}

/// 只接受 `hierarchy` / `maps`（与 migration CHECK 双保险）。
pub async fn set_zone_source(pool: &SqlitePool, workspace_id: &str, source: &str) -> Result<bool, sqlx::Error> {
    if !matches!(source, "hierarchy" | "maps") {
        return Err(sqlx::Error::Configuration(
            format!("invalid zone_source: {source}").into(),
        ));
    }
    let r = sqlx::query("UPDATE workspaces SET zone_source = ? WHERE id = ?")
        .bind(source)
        .bind(workspace_id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() > 0)
}

// ── Db facade ──────────────────────────────────────────

impl Db {
    pub async fn list_maps(&self, workspace_id: &str) -> Result<Vec<MapRecord>, sqlx::Error> {
        list_maps(self.pool(), workspace_id).await
    }

    pub async fn find_map(&self, workspace_id: &str, id: &str) -> Result<Option<MapRecord>, sqlx::Error> {
        find_map(self.pool(), workspace_id, id).await
    }

    pub async fn find_default_map(&self, workspace_id: &str) -> Result<Option<MapRecord>, sqlx::Error> {
        find_default_map(self.pool(), workspace_id).await
    }

    pub async fn upsert_map(&self, rec: &MapRecord) -> Result<(), sqlx::Error> {
        upsert_map(self.pool(), rec).await
    }

    pub async fn delete_map(&self, workspace_id: &str, id: &str) -> Result<bool, sqlx::Error> {
        delete_map(self.pool(), workspace_id, id).await
    }

    pub async fn list_places(&self, workspace_id: &str, map_id: &str) -> Result<Vec<PlaceRecord>, sqlx::Error> {
        list_places(self.pool(), workspace_id, map_id).await
    }

    pub async fn find_place(
        &self,
        workspace_id: &str,
        map_id: &str,
        id: &str,
    ) -> Result<Option<PlaceRecord>, sqlx::Error> {
        find_place(self.pool(), workspace_id, map_id, id).await
    }

    pub async fn upsert_place(&self, workspace_id: &str, rec: &PlaceRecord) -> Result<(), sqlx::Error> {
        upsert_place(self.pool(), workspace_id, rec).await
    }

    pub async fn delete_place(&self, workspace_id: &str, map_id: &str, id: &str) -> Result<bool, sqlx::Error> {
        delete_place(self.pool(), workspace_id, map_id, id).await
    }

    pub async fn get_thing_position(
        &self,
        workspace_id: &str,
        thing_id: &str,
    ) -> Result<Option<ThingPosition>, sqlx::Error> {
        get_thing_position(self.pool(), workspace_id, thing_id).await
    }

    pub async fn set_thing_position(
        &self,
        workspace_id: &str,
        thing_id: &str,
        position: &ThingPosition,
    ) -> Result<bool, sqlx::Error> {
        set_thing_position(self.pool(), workspace_id, thing_id, position).await
    }

    pub async fn list_thing_positions(&self, workspace_id: &str) -> Result<Vec<(String, ThingPosition)>, sqlx::Error> {
        list_thing_positions(self.pool(), workspace_id).await
    }

    pub async fn set_thing_reading_position(
        &self,
        workspace_id: &str,
        thing_id: &str,
        position: &ThingPosition,
    ) -> Result<bool, sqlx::Error> {
        set_thing_reading_position(self.pool(), workspace_id, thing_id, position).await
    }

    pub async fn get_thing_zone_ref(&self, workspace_id: &str, thing_id: &str) -> Result<Option<String>, sqlx::Error> {
        get_thing_zone_ref(self.pool(), workspace_id, thing_id).await
    }

    pub async fn set_thing_zone_ref(
        &self,
        workspace_id: &str,
        thing_id: &str,
        zone_ref: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        set_thing_zone_ref(self.pool(), workspace_id, thing_id, zone_ref).await
    }

    pub async fn list_thing_zone_refs(&self, workspace_id: &str) -> Result<Vec<(String, String)>, sqlx::Error> {
        list_thing_zone_refs(self.pool(), workspace_id).await
    }

    pub async fn get_zone_source(&self, workspace_id: &str) -> Result<Option<String>, sqlx::Error> {
        get_zone_source(self.pool(), workspace_id).await
    }

    pub async fn set_zone_source(&self, workspace_id: &str, source: &str) -> Result<bool, sqlx::Error> {
        set_zone_source(self.pool(), workspace_id, source).await
    }

    pub async fn find_place_by_name(
        &self,
        workspace_id: &str,
        name: &str,
        kind: Option<PlaceKind>,
    ) -> Result<Option<PlaceRecord>, sqlx::Error> {
        find_place_by_name(self.pool(), workspace_id, name, kind).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::test_pool;

    async fn setup() -> Db {
        let db = Db::new(test_pool().await);
        crate::seed::seed_system(&db).await.unwrap();
        for ws in ["ws1", "ws2"] {
            sqlx::query(
                "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
                 VALUES (?, ?, 'tenant-default-001', '2026-10-22', '2026-10-22')",
            )
            .bind(ws)
            .bind(ws)
            .execute(db.pool())
            .await
            .unwrap();
        }
        db
    }

    async fn insert_thing(db: &Db, id: &str, ws: &str) {
        sqlx::query(
            "INSERT INTO things (id, name, thing_type, state, workspace_id, created_at, updated_at) \
             VALUES (?, ?, 'device', 0, ?, '2026-10-22', '2026-10-22')",
        )
        .bind(id)
        .bind(id)
        .bind(ws)
        .execute(db.pool())
        .await
        .unwrap();
    }

    fn map_rec(id: &str, ws: &str, name: &str, is_default: bool) -> MapRecord {
        MapRecord {
            id: id.into(),
            workspace_id: ws.into(),
            name: name.into(),
            bounds: MapBounds {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 60.0,
            },
            anchor_map_id: None,
            is_default,
            created_at: "2026-10-22T00:00:00Z".into(),
            updated_at: "2026-10-22T00:00:00Z".into(),
        }
    }

    fn place_rec(id: &str, map_id: &str, name: &str, kind: PlaceKind, geometry: Geometry) -> PlaceRecord {
        PlaceRecord {
            id: id.into(),
            map_id: map_id.into(),
            name: name.into(),
            kind,
            geometry,
            heading: Some(90.0),
            created_at: "2026-10-22T00:00:00Z".into(),
            updated_at: "2026-10-22T00:00:00Z".into(),
        }
    }

    fn zone_geometry() -> Geometry {
        Geometry::Polygon {
            points: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
        }
    }

    #[tokio::test]
    async fn map_and_place_crud_round_trip() {
        let db = setup().await;
        let m = map_rec("m1", "ws1", "floor-1", true);
        db.upsert_map(&m).await.unwrap();
        assert_eq!(db.find_map("ws1", "m1").await.unwrap(), Some(m.clone()));
        assert_eq!(db.list_maps("ws1").await.unwrap(), vec![m.clone()]);

        let p = place_rec("p1", "m1", "kitchen", PlaceKind::Zone, zone_geometry());
        db.upsert_place("ws1", &p).await.unwrap();
        assert_eq!(db.find_place("ws1", "m1", "p1").await.unwrap(), Some(p.clone()));
        assert_eq!(db.list_places("ws1", "m1").await.unwrap(), vec![p.clone()]);
        assert_eq!(
            db.find_place_by_name("ws1", "kitchen", None).await.unwrap(),
            Some(p.clone())
        );

        // point geometry 也走 JSON round-trip
        let landmark = place_rec(
            "p2",
            "m1",
            "door",
            PlaceKind::Landmark,
            Geometry::Point { x: 1.5, y: 2.5 },
        );
        db.upsert_place("ws1", &landmark).await.unwrap();
        assert_eq!(db.find_place("ws1", "m1", "p2").await.unwrap(), Some(landmark));

        assert!(db.delete_place("ws1", "m1", "p1").await.unwrap());
        assert!(db.find_place("ws1", "m1", "p1").await.unwrap().is_none());
        assert!(db.delete_map("ws1", "m1").await.unwrap());
        assert!(db.find_map("ws1", "m1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn map_names_unique_per_workspace() {
        let db = setup().await;
        db.upsert_map(&map_rec("m1", "ws1", "floor-1", false)).await.unwrap();
        let err = db
            .upsert_map(&map_rec("m2", "ws1", "floor-1", false))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("UNIQUE"), "unexpected: {err}");
        // 不同工作区可以同名
        db.upsert_map(&map_rec("m3", "ws2", "floor-1", false)).await.unwrap();
    }

    #[tokio::test]
    async fn place_names_unique_per_map() {
        let db = setup().await;
        db.upsert_map(&map_rec("m1", "ws1", "floor-1", true)).await.unwrap();
        let p = place_rec("p1", "m1", "kitchen", PlaceKind::Zone, zone_geometry());
        db.upsert_place("ws1", &p).await.unwrap();
        let mut dup = p.clone();
        dup.id = "p2".into();
        let err = db.upsert_place("ws1", &dup).await.unwrap_err();
        assert!(err.to_string().contains("UNIQUE"), "unexpected: {err}");
    }

    #[tokio::test]
    async fn default_map_is_single_and_switchable() {
        let db = setup().await;
        db.upsert_map(&map_rec("m1", "ws1", "floor-1", true)).await.unwrap();
        db.upsert_map(&map_rec("m2", "ws1", "floor-2", true)).await.unwrap();

        let default = db.find_default_map("ws1").await.unwrap().unwrap();
        assert_eq!(default.id, "m2");
        let m1 = db.find_map("ws1", "m1").await.unwrap().unwrap();
        assert!(!m1.is_default);
        assert!(db.find_default_map("ws2").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn thing_position_upsert_round_trip() {
        let db = setup().await;
        db.upsert_map(&map_rec("m1", "ws1", "floor-1", true)).await.unwrap();
        insert_thing(&db, "t1", "ws1").await;
        assert_eq!(db.get_thing_position("ws1", "t1").await.unwrap().unwrap().x, None);

        let pos = ThingPosition {
            map_id: Some("m1".into()),
            x: Some(3.5),
            y: Some(-2.0),
            level: Some(2),
            heading: Some(45.0),
            trust: LocationTrust::Trusted,
            updated_at: Some("2026-10-22T00:00:00Z".into()),
        };
        assert!(db.set_thing_position("ws1", "t1", &pos).await.unwrap());
        assert_eq!(db.get_thing_position("ws1", "t1").await.unwrap(), Some(pos.clone()));

        // 覆盖写
        let mut next = pos.clone();
        next.x = Some(4.0);
        next.trust = LocationTrust::Lost;
        db.set_thing_position("ws1", "t1", &next).await.unwrap();
        assert_eq!(db.get_thing_position("ws1", "t1").await.unwrap(), Some(next));

        // 跨工作区不可见；外部工作区写坐标（map m1 属 ws1）被拒绝
        assert!(db.get_thing_position("ws2", "t1").await.unwrap().is_none());
        assert!(db.set_thing_position("ws2", "t1", &pos).await.is_err());
    }

    #[tokio::test]
    async fn zone_source_defaults_hierarchy_and_switches() {
        let db = setup().await;
        assert_eq!(db.get_zone_source("ws1").await.unwrap().as_deref(), Some("hierarchy"));
        assert!(db.set_zone_source("ws1", "maps").await.unwrap());
        assert_eq!(db.get_zone_source("ws1").await.unwrap().as_deref(), Some("maps"));
        assert!(!db.set_zone_source("missing", "maps").await.unwrap());
        assert_eq!(db.get_zone_source("missing").await.unwrap(), None);
    }

    #[tokio::test]
    async fn workspace_scoping_isolates_maps_places_positions() {
        let db = setup().await;
        db.upsert_map(&map_rec("m1", "ws1", "floor-1", true)).await.unwrap();
        db.upsert_place(
            "ws1",
            &place_rec("p1", "m1", "kitchen", PlaceKind::Zone, zone_geometry()),
        )
        .await
        .unwrap();

        assert!(db.list_maps("ws2").await.unwrap().is_empty());
        assert!(db.find_map("ws2", "m1").await.unwrap().is_none());
        assert!(!db.delete_map("ws2", "m1").await.unwrap());
        // place 通过 map 归属工作区
        assert!(db.find_place_by_name("ws2", "kitchen", None).await.unwrap().is_none());
        assert!(db.find_place_by_name("ws1", "kitchen", None).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn place_reads_and_writes_are_workspace_scoped() {
        let db = setup().await;
        db.upsert_map(&map_rec("m1", "ws1", "floor-1", true)).await.unwrap();
        db.upsert_map(&map_rec("m2", "ws2", "floor-1", true)).await.unwrap();
        let p = place_rec("p2", "m2", "kitchen", PlaceKind::Zone, zone_geometry());
        db.upsert_place("ws2", &p).await.unwrap();

        // ws1 不能通过 ws2 的 map_id 列举/读取/删除地点
        assert!(db.list_places("ws1", "m2").await.unwrap().is_empty());
        assert!(db.find_place("ws1", "m2", "p2").await.unwrap().is_none());
        assert!(!db.delete_place("ws1", "m2", "p2").await.unwrap());
        assert!(db.find_place("ws2", "m2", "p2").await.unwrap().is_some());

        // 向外部 map 插入地点被拒绝
        let foreign = place_rec("p3", "m2", "garage", PlaceKind::Zone, zone_geometry());
        assert!(db.upsert_place("ws1", &foreign).await.is_err());
        assert!(db.find_place("ws2", "m2", "p3").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn upserts_cannot_rehome_across_workspaces() {
        let db = setup().await;
        db.upsert_map(&map_rec("m1", "ws1", "floor-1", true)).await.unwrap();
        db.upsert_map(&map_rec("m2", "ws2", "floor-1", true)).await.unwrap();

        // map id 跨工作区冲突被拒绝，原行不变
        let mut rehome = map_rec("m1", "ws2", "stolen", false);
        rehome.bounds.x = 9.0;
        assert!(db.upsert_map(&rehome).await.is_err());
        let original = db.find_map("ws1", "m1").await.unwrap().unwrap();
        assert_eq!(original.name, "floor-1");
        assert!(db.find_map("ws2", "m1").await.unwrap().is_none());

        // place id 跨工作区冲突被拒绝
        db.upsert_place(
            "ws1",
            &place_rec("p1", "m1", "kitchen", PlaceKind::Zone, zone_geometry()),
        )
        .await
        .unwrap();
        let stolen = place_rec("p1", "m2", "kitchen", PlaceKind::Zone, zone_geometry());
        assert!(db.upsert_place("ws2", &stolen).await.is_err());
        let original = db.find_place("ws1", "m1", "p1").await.unwrap().unwrap();
        assert_eq!(original.map_id, "m1");
        assert!(db.find_place("ws2", "m2", "p1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn set_position_rejects_foreign_or_missing_map() {
        let db = setup().await;
        db.upsert_map(&map_rec("m1", "ws1", "floor-1", true)).await.unwrap();
        db.upsert_map(&map_rec("m2", "ws2", "floor-1", true)).await.unwrap();
        insert_thing(&db, "t1", "ws1").await;

        let mut pos = ThingPosition {
            map_id: Some("m2".into()),
            x: Some(1.0),
            y: Some(2.0),
            ..Default::default()
        };
        assert!(db.set_thing_position("ws1", "t1", &pos).await.is_err());
        assert_eq!(
            db.get_thing_position("ws1", "t1").await.unwrap().unwrap(),
            ThingPosition::default()
        );

        pos.map_id = Some("missing".into());
        assert!(db.set_thing_position("ws1", "t1", &pos).await.is_err());

        pos.map_id = Some("m1".into());
        assert!(db.set_thing_position("ws1", "t1", &pos).await.unwrap());
    }

    #[tokio::test]
    async fn zone_source_rejects_unknown_values_and_new_workspaces_use_maps() {
        let db = setup().await;
        assert!(db.set_zone_source("ws1", "bogus").await.is_err());
        assert_eq!(db.get_zone_source("ws1").await.unwrap().as_deref(), Some("hierarchy"));

        // CHECK 约束兜底（绕过 app 校验的直写也被拒）
        let err = sqlx::query("UPDATE workspaces SET zone_source = 'bogus' WHERE id = 'ws1'")
            .execute(db.pool())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("CHECK"), "unexpected: {err}");

        // 新建工作区默认 maps；存量工作区保持 hierarchy
        let ws = db
            .create_workspace("tenant-default-001", "new-ws", None, None, None)
            .await
            .unwrap();
        assert_eq!(db.get_zone_source(&ws.id).await.unwrap().as_deref(), Some("maps"));
    }

    #[tokio::test]
    async fn zone_ref_round_trip_and_workspace_isolation() {
        let db = setup().await;
        insert_thing(&db, "t1", "ws1").await;
        insert_thing(&db, "t2", "ws2").await;

        assert_eq!(db.get_thing_zone_ref("ws1", "t1").await.unwrap(), None);
        assert!(db.set_thing_zone_ref("ws1", "t1", Some("车间A")).await.unwrap());
        assert_eq!(
            db.get_thing_zone_ref("ws1", "t1").await.unwrap().as_deref(),
            Some("车间A")
        );
        assert_eq!(
            db.list_thing_zone_refs("ws1").await.unwrap(),
            vec![("t1".into(), "车间A".into())]
        );

        // 跨工作区不可见/不可写
        assert_eq!(db.get_thing_zone_ref("ws2", "t1").await.unwrap(), None);
        assert!(!db.set_thing_zone_ref("ws2", "t1", Some("stolen")).await.unwrap());
        assert_eq!(
            db.get_thing_zone_ref("ws1", "t1").await.unwrap().as_deref(),
            Some("车间A")
        );
        assert!(db.list_thing_zone_refs("ws2").await.unwrap().is_empty());

        // 清除
        assert!(db.set_thing_zone_ref("ws1", "t1", None).await.unwrap());
        assert_eq!(db.get_thing_zone_ref("ws1", "t1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn list_positions_batches_and_reading_write_preserves_map() {
        let db = setup().await;
        db.upsert_map(&map_rec("m1", "ws1", "floor-1", true)).await.unwrap();
        insert_thing(&db, "t1", "ws1").await;
        insert_thing(&db, "t2", "ws1").await;
        insert_thing(&db, "t3", "ws2").await;

        // 先由 placement 写 map_id，再由驱动读数写坐标：map_id 必须保留。
        let placed = ThingPosition {
            map_id: Some("m1".into()),
            x: Some(1.0),
            y: Some(2.0),
            ..Default::default()
        };
        db.set_thing_position("ws1", "t1", &placed).await.unwrap();
        let reading = ThingPosition {
            map_id: None,
            x: Some(9.0),
            y: Some(8.0),
            level: Some(2),
            heading: Some(45.0),
            trust: LocationTrust::Lost,
            updated_at: Some("2026-10-22T12:00:00Z".into()),
        };
        assert!(db.set_thing_reading_position("ws1", "t1", &reading).await.unwrap());
        let got = db.get_thing_position("ws1", "t1").await.unwrap().unwrap();
        assert_eq!(got.map_id.as_deref(), Some("m1"), "读数不得清空 map_id");
        assert_eq!((got.x, got.y), (Some(9.0), Some(8.0)));
        assert_eq!(got.trust, LocationTrust::Lost);

        let all = db.list_thing_positions("ws1").await.unwrap();
        assert_eq!(all.len(), 2, "只返回本工作区的物: {all:?}");
        assert_eq!(all[0].0, "t1");
        assert_eq!(all[1].0, "t2");
        assert!(all[0].1.x.is_some());
        assert!(all[1].1.x.is_none());
        assert_eq!(db.list_thing_positions("ws2").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn find_place_by_name_filters_kind_for_zone_ref() {
        let db = setup().await;
        db.upsert_map(&map_rec("m1", "ws1", "floor-1", true)).await.unwrap();
        db.upsert_map(&map_rec("m2", "ws1", "floor-2", false)).await.unwrap();
        db.upsert_place(
            "ws1",
            &place_rec(
                "p1",
                "m1",
                "gate",
                PlaceKind::Landmark,
                Geometry::Point { x: 0.0, y: 0.0 },
            ),
        )
        .await
        .unwrap();
        db.upsert_place("ws1", &place_rec("p2", "m2", "gate", PlaceKind::Zone, zone_geometry()))
            .await
            .unwrap();

        let zone = db
            .find_place_by_name("ws1", "gate", Some(PlaceKind::Zone))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(zone.id, "p2");
        assert_eq!(zone.kind, PlaceKind::Zone);

        let landmark = db
            .find_place_by_name("ws1", "gate", Some(PlaceKind::Landmark))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(landmark.id, "p1");

        // 不带过滤时默认图优先
        assert_eq!(
            db.find_place_by_name("ws1", "gate", None).await.unwrap().unwrap().id,
            "p1"
        );
    }
}
