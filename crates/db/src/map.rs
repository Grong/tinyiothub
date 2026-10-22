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
pub async fn upsert_map(pool: &SqlitePool, rec: &MapRecord) -> Result<(), sqlx::Error> {
    let bounds_json = serde_json::to_string(&rec.bounds).map_err(encode_err)?;
    let mut tx = pool.begin().await?;
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
             workspace_id = excluded.workspace_id,
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

pub async fn list_places(pool: &SqlitePool, map_id: &str) -> Result<Vec<PlaceRecord>, sqlx::Error> {
    let rows: Vec<PlaceRow> = sqlx::query_as(
        "SELECT id, map_id, name, kind, geometry_json, heading, created_at, updated_at
         FROM places WHERE map_id = ? ORDER BY name ASC",
    )
    .bind(map_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(PlaceRecord::try_from).collect()
}

pub async fn find_place(pool: &SqlitePool, map_id: &str, id: &str) -> Result<Option<PlaceRecord>, sqlx::Error> {
    let row: Option<PlaceRow> = sqlx::query_as(
        "SELECT id, map_id, name, kind, geometry_json, heading, created_at, updated_at
         FROM places WHERE map_id = ? AND id = ?",
    )
    .bind(map_id)
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.map(PlaceRecord::try_from).transpose()
}

pub async fn upsert_place(pool: &SqlitePool, rec: &PlaceRecord) -> Result<(), sqlx::Error> {
    let geometry_json = serde_json::to_string(&rec.geometry).map_err(encode_err)?;
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
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_place(pool: &SqlitePool, map_id: &str, id: &str) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("DELETE FROM places WHERE map_id = ? AND id = ?")
        .bind(map_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() > 0)
}

/// 按名称在工作区内找地点：默认图优先（zoneRef 名称解析）。
pub async fn find_place_by_name(
    pool: &SqlitePool,
    workspace_id: &str,
    name: &str,
) -> Result<Option<PlaceRecord>, sqlx::Error> {
    let row: Option<PlaceRow> = sqlx::query_as(
        "SELECT p.id, p.map_id, p.name, p.kind, p.geometry_json, p.heading, p.created_at, p.updated_at
         FROM places p JOIN maps m ON m.id = p.map_id
         WHERE m.workspace_id = ? AND p.name = ?
         ORDER BY m.is_default DESC, p.created_at ASC
         LIMIT 1",
    )
    .bind(workspace_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;
    row.map(PlaceRecord::try_from).transpose()
}

// ── 物坐标 ─────────────────────────────────────────────

#[derive(Debug, FromRow)]
struct PositionRow {
    map_id: Option<String>,
    position_x: Option<f64>,
    position_y: Option<f64>,
    position_level: Option<i64>,
    position_heading: Option<f64>,
    position_trust: Option<String>,
    position_updated_at: Option<String>,
}

pub async fn get_thing_position(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: &str,
) -> Result<Option<ThingPosition>, sqlx::Error> {
    let row: Option<PositionRow> = sqlx::query_as(
        "SELECT map_id, position_x, position_y, position_level, position_heading,
                position_trust, position_updated_at
         FROM things WHERE id = ? AND workspace_id = ?",
    )
    .bind(thing_id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ThingPosition {
        map_id: r.map_id,
        x: r.position_x,
        y: r.position_y,
        level: r.position_level,
        heading: r.position_heading,
        trust: LocationTrust::from_db(r.position_trust.as_deref()),
        updated_at: r.position_updated_at,
    }))
}

/// 写回物坐标列（同一 workspace 才命中）。返回是否找到该物。
pub async fn set_thing_position(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: &str,
    position: &ThingPosition,
) -> Result<bool, sqlx::Error> {
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
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

// ── zone_source ────────────────────────────────────────

pub async fn get_zone_source(pool: &SqlitePool, workspace_id: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT zone_source FROM workspaces WHERE id = ?")
        .bind(workspace_id)
        .fetch_optional(pool)
        .await
}

pub async fn set_zone_source(pool: &SqlitePool, workspace_id: &str, source: &str) -> Result<bool, sqlx::Error> {
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

    pub async fn list_places(&self, map_id: &str) -> Result<Vec<PlaceRecord>, sqlx::Error> {
        list_places(self.pool(), map_id).await
    }

    pub async fn find_place(&self, map_id: &str, id: &str) -> Result<Option<PlaceRecord>, sqlx::Error> {
        find_place(self.pool(), map_id, id).await
    }

    pub async fn upsert_place(&self, rec: &PlaceRecord) -> Result<(), sqlx::Error> {
        upsert_place(self.pool(), rec).await
    }

    pub async fn delete_place(&self, map_id: &str, id: &str) -> Result<bool, sqlx::Error> {
        delete_place(self.pool(), map_id, id).await
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

    pub async fn get_zone_source(&self, workspace_id: &str) -> Result<Option<String>, sqlx::Error> {
        get_zone_source(self.pool(), workspace_id).await
    }

    pub async fn set_zone_source(&self, workspace_id: &str, source: &str) -> Result<bool, sqlx::Error> {
        set_zone_source(self.pool(), workspace_id, source).await
    }

    pub async fn find_place_by_name(&self, workspace_id: &str, name: &str) -> Result<Option<PlaceRecord>, sqlx::Error> {
        find_place_by_name(self.pool(), workspace_id, name).await
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
        db.upsert_place(&p).await.unwrap();
        assert_eq!(db.find_place("m1", "p1").await.unwrap(), Some(p.clone()));
        assert_eq!(db.list_places("m1").await.unwrap(), vec![p.clone()]);
        assert_eq!(db.find_place_by_name("ws1", "kitchen").await.unwrap(), Some(p.clone()));

        // point geometry 也走 JSON round-trip
        let landmark = place_rec(
            "p2",
            "m1",
            "door",
            PlaceKind::Landmark,
            Geometry::Point { x: 1.5, y: 2.5 },
        );
        db.upsert_place(&landmark).await.unwrap();
        assert_eq!(db.find_place("m1", "p2").await.unwrap(), Some(landmark));

        assert!(db.delete_place("m1", "p1").await.unwrap());
        assert!(db.find_place("m1", "p1").await.unwrap().is_none());
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
        db.upsert_place(&p).await.unwrap();
        let mut dup = p.clone();
        dup.id = "p2".into();
        let err = db.upsert_place(&dup).await.unwrap_err();
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

        // 跨工作区不可见/不可写
        assert!(db.get_thing_position("ws2", "t1").await.unwrap().is_none());
        assert!(!db.set_thing_position("ws2", "t1", &pos).await.unwrap());
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
        db.upsert_place(&place_rec("p1", "m1", "kitchen", PlaceKind::Zone, zone_geometry()))
            .await
            .unwrap();

        assert!(db.list_maps("ws2").await.unwrap().is_empty());
        assert!(db.find_map("ws2", "m1").await.unwrap().is_none());
        assert!(!db.delete_map("ws2", "m1").await.unwrap());
        // place 通过 map 归属工作区
        assert!(db.find_place_by_name("ws2", "kitchen").await.unwrap().is_none());
        assert!(db.find_place_by_name("ws1", "kitchen").await.unwrap().is_some());
    }
}
