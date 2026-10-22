//! E4d `/v1/maps` CRUD + `/v1/maps/{id}/places` CRUD.
//!
//! - 读取：JWT 工作区作用域（`WorkspaceScope`）。
//! - 写入：admin 闸门（与 driver install 同一角色来源，失败即 closed）。
//! - 校验：bounds w/h > 0 且有限；名称非空且工作区/地图内唯一；zone 必须是
//!   ≥3 点的有限多边形；landmark 必须是有限点；默认图唯一（upsert 事务内
//!   清旧默认）；锚图必须属于同一工作区。
//! - 写成功后调用 `ZoneService::invalidate`，让 DataServer / watch 立即看到
//!   新地图与地点（5s TTL 不再掩盖 CRUD）。

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use tinyiothub_core::models::spatial::{Geometry, MapBounds, MapRecord, PlaceKind, PlaceRecord};
use tinyiothub_runtime::maps::polygon_area;
use tinyiothub_storage::Db;
use tinyiothub_web::middleware::workspace::{AuthClaims, WorkspaceScope};
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};

use crate::state::AppState;

/// 成功体与错误体形状一致；`?` 用于 admin/scope/storage 前置失败。
type ApiSuccess = (StatusCode, Json<ApiResponse<Value>>);
type ApiError = (StatusCode, Json<ApiResponse<Value>>);
type ApiResult = Result<ApiSuccess, ApiError>;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_maps).post(create_map))
        .route("/{id}", get(get_map).patch(update_map).delete(delete_map))
        .route("/{id}/places", get(list_places).post(create_place))
        .route("/{id}/places/{place_id}", patch(update_place).delete(delete_place))
}

// ── 响应 / 错误 ────────────────────────────────────────

fn ok(data: impl Serialize) -> ApiResult {
    Ok((
        StatusCode::OK,
        ApiResponseBuilder::success(serde_json::to_value(data).unwrap_or(Value::Null)),
    ))
}

fn created(data: impl Serialize) -> ApiSuccess {
    (
        StatusCode::CREATED,
        ApiResponseBuilder::success(serde_json::to_value(data).unwrap_or(Value::Null)),
    )
}

fn fail(status: StatusCode, message: impl Into<String>) -> ApiError {
    (
        status,
        ApiResponseBuilder::error_with_code(status.as_u16() as i32, message),
    )
}

fn db_error(context: &str, error: impl std::fmt::Display) -> ApiError {
    tracing::error!(error = %error, context, "map API storage error");
    fail(StatusCode::INTERNAL_SERVER_ERROR, "storage error")
}

/// SQLite UNIQUE 约束（并发写绕过预检查）：`maps(workspace_id,name)` /
/// `places(map_id,name)` 冲突必须映射为 409，而不是 500。
fn is_unique_violation(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(db) => db.is_unique_violation() || db.message().contains("UNIQUE constraint failed"),
        _ => false,
    }
}

fn write_error(context: &str, error: sqlx::Error, conflict: &str) -> ApiError {
    if is_unique_violation(&error) {
        fail(StatusCode::CONFLICT, conflict)
    } else {
        db_error(context, error)
    }
}

/// Admin gate（镜像 driver install）：角色读取失败按 500，未命中按 403。
async fn ensure_admin(state: &AppState, user_id: &str) -> Result<(), ApiError> {
    match state.db.count_user_admin_roles(user_id).await {
        Ok(n) if n > 0 => Ok(()),
        Ok(_) => Err(fail(StatusCode::FORBIDDEN, "Access denied: admin role required")),
        Err(e) => {
            tracing::warn!(error = %e, user = %user_id, "admin role check failed");
            Err(fail(StatusCode::INTERNAL_SERVER_ERROR, "Permission check failed"))
        }
    }
}

/// 写入必须有具体工作区（空 = 内置槽，不允许管理面落入）。
fn scoped_workspace(ws: Option<String>) -> Result<String, ApiError> {
    match ws {
        Some(ws) if !ws.trim().is_empty() => Ok(ws),
        _ => Err(fail(
            StatusCode::FORBIDDEN,
            "workspace id is required for map management",
        )),
    }
}

fn invalidate_zones(state: &AppState, workspace_id: &str) {
    if let Some(zones) = &state.zones {
        zones.invalidate(workspace_id);
    }
}

// ── 视图 / 请求体 ──────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MapView {
    id: String,
    workspace_id: String,
    name: String,
    bounds: MapBounds,
    anchor_map_id: Option<String>,
    is_default: bool,
    created_at: String,
    updated_at: String,
}

impl From<MapRecord> for MapView {
    fn from(rec: MapRecord) -> Self {
        Self {
            id: rec.id,
            workspace_id: rec.workspace_id,
            name: rec.name,
            bounds: rec.bounds,
            anchor_map_id: rec.anchor_map_id,
            is_default: rec.is_default,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaceView {
    id: String,
    map_id: String,
    name: String,
    kind: PlaceKind,
    geometry: Geometry,
    heading: Option<f64>,
    created_at: String,
    updated_at: String,
}

impl From<PlaceRecord> for PlaceView {
    fn from(rec: PlaceRecord) -> Self {
        Self {
            id: rec.id,
            map_id: rec.map_id,
            name: rec.name,
            kind: rec.kind,
            geometry: rec.geometry,
            heading: rec.heading,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMapRequest {
    pub name: String,
    pub bounds: MapBounds,
    #[serde(default)]
    pub anchor_map_id: Option<String>,
    #[serde(default)]
    pub is_default: bool,
}

/// 三态反序列化：省略 → `None`，显式 `null` → `Some(None)`，值 →
/// `Some(Some(v))`。`Option<T>` 默认无法区分“省略”与 `null`。
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMapRequest {
    pub name: Option<String>,
    pub bounds: Option<MapBounds>,
    /// PATCH 语义：省略 = 不变；`null` = 清空锚定。
    #[serde(default, deserialize_with = "double_option")]
    pub anchor_map_id: Option<Option<String>>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePlaceRequest {
    pub name: String,
    pub kind: PlaceKind,
    pub geometry: Geometry,
    pub heading: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePlaceRequest {
    pub name: Option<String>,
    pub kind: Option<PlaceKind>,
    pub geometry: Option<Geometry>,
    /// PATCH 语义：省略 = 不变；`null` = 清空朝向。
    #[serde(default, deserialize_with = "double_option")]
    pub heading: Option<Option<f64>>,
}

// ── 校验 ──────────────────────────────────────────────

fn validate_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(fail(StatusCode::BAD_REQUEST, "name must not be empty"));
    }
    Ok(name.to_string())
}

fn validate_bounds(bounds: &MapBounds) -> Result<(), ApiError> {
    if ![bounds.x, bounds.y, bounds.w, bounds.h].iter().all(|v| v.is_finite()) {
        return Err(fail(StatusCode::BAD_REQUEST, "bounds must be finite"));
    }
    if bounds.w <= 0.0 || bounds.h <= 0.0 {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            "bounds width and height must be positive",
        ));
    }
    // x+w / y+h 溢出成 inf 会让 within_bounds 恒真；显式拒绝。
    if !(bounds.x + bounds.w).is_finite() || !(bounds.y + bounds.h).is_finite() {
        return Err(fail(StatusCode::BAD_REQUEST, "bounds must not overflow"));
    }
    Ok(())
}

fn validate_heading(heading: Option<f64>) -> Result<(), ApiError> {
    if heading.is_some_and(|h| !h.is_finite()) {
        return Err(fail(StatusCode::BAD_REQUEST, "heading must be finite"));
    }
    Ok(())
}

/// zone ⇒ ≥3 点有限多边形；landmark ⇒ 有限点（spec §2.1 / Task 5）。
fn validate_geometry(kind: PlaceKind, geometry: &Geometry) -> Result<(), ApiError> {
    match (kind, geometry) {
        (PlaceKind::Zone, Geometry::Polygon { points }) => {
            if points.len() < 3 {
                return Err(fail(
                    StatusCode::BAD_REQUEST,
                    "zone geometry must be a polygon with at least 3 points",
                ));
            }
            if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
                return Err(fail(StatusCode::BAD_REQUEST, "polygon coordinates must be finite"));
            }
            if points.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(fail(
                    StatusCode::BAD_REQUEST,
                    "polygon must not contain duplicate consecutive points",
                ));
            }
            if polygon_area(points) <= 0.0 {
                return Err(fail(StatusCode::BAD_REQUEST, "zone polygon must have non-zero area"));
            }
            Ok(())
        }
        (PlaceKind::Zone, _) => Err(fail(StatusCode::BAD_REQUEST, "zone geometry must be a polygon")),
        (PlaceKind::Landmark, Geometry::Point { x, y }) if x.is_finite() && y.is_finite() => Ok(()),
        (PlaceKind::Landmark, Geometry::Point { .. }) => {
            Err(fail(StatusCode::BAD_REQUEST, "point coordinates must be finite"))
        }
        (PlaceKind::Landmark, _) => Err(fail(StatusCode::BAD_REQUEST, "landmark geometry must be a point")),
    }
}

async fn validate_anchor(
    db: &Db,
    workspace_id: &str,
    anchor: Option<&str>,
    self_id: Option<&str>,
) -> Result<(), ApiError> {
    let Some(anchor) = anchor else {
        return Ok(());
    };
    if Some(anchor) == self_id {
        return Err(fail(StatusCode::BAD_REQUEST, "map cannot anchor to itself"));
    }
    match db.find_map(workspace_id, anchor).await {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(fail(
            StatusCode::BAD_REQUEST,
            "anchor map does not exist in this workspace",
        )),
        Err(e) => Err(db_error("find anchor map", e)),
    }
}

async fn map_name_in_use(db: &Db, workspace_id: &str, name: &str, except_id: Option<&str>) -> Result<bool, ApiError> {
    let maps = db
        .list_maps(workspace_id)
        .await
        .map_err(|e| db_error("list maps for name check", e))?;
    Ok(maps
        .iter()
        .any(|map| map.name == name && except_id.is_none_or(|id| map.id != id)))
}

fn place_name_in_use(places: &[PlaceRecord], name: &str, except_id: Option<&str>) -> bool {
    places
        .iter()
        .any(|place| place.name == name && except_id.is_none_or(|id| place.id != id))
}

// ── Maps ──────────────────────────────────────────────

async fn list_maps(State(state): State<AppState>, WorkspaceScope(ws): WorkspaceScope) -> ApiResult {
    let ws = ws.unwrap_or_default();
    match state.db.list_maps(&ws).await {
        Ok(maps) => ok(maps.into_iter().map(MapView::from).collect::<Vec<_>>()),
        Err(e) => Err(db_error("list maps", e)),
    }
}

async fn get_map(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Path(id): Path<String>,
) -> ApiResult {
    let ws = ws.unwrap_or_default();
    match state.db.find_map(&ws, &id).await {
        Ok(Some(map)) => ok(MapView::from(map)),
        Ok(None) => Err(fail(StatusCode::NOT_FOUND, "map not found")),
        Err(e) => Err(db_error("get map", e)),
    }
}

async fn create_map(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    WorkspaceScope(ws): WorkspaceScope,
    Json(req): Json<CreateMapRequest>,
) -> ApiResult {
    ensure_admin(&state, &claims.user_id).await?;
    let ws = scoped_workspace(ws)?;

    let name = validate_name(&req.name)?;
    validate_bounds(&req.bounds)?;
    validate_anchor(&state.db, &ws, req.anchor_map_id.as_deref(), None).await?;
    if map_name_in_use(&state.db, &ws, &name, None).await? {
        return Err(fail(StatusCode::CONFLICT, "map name already exists in workspace"));
    }

    let now = tinyiothub_core::now_string();
    let rec = MapRecord {
        id: uuid::Uuid::new_v4().to_string(),
        workspace_id: ws.clone(),
        name,
        bounds: req.bounds,
        anchor_map_id: req.anchor_map_id,
        is_default: req.is_default,
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .db
        .upsert_map(&rec)
        .await
        .map_err(|e| write_error("create map", e, "map name already exists in workspace"))?;
    invalidate_zones(&state, &ws);
    Ok(created(MapView::from(rec)))
}

async fn update_map(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    WorkspaceScope(ws): WorkspaceScope,
    Path(id): Path<String>,
    Json(req): Json<UpdateMapRequest>,
) -> ApiResult {
    ensure_admin(&state, &claims.user_id).await?;
    let ws = scoped_workspace(ws)?;

    let Some(mut rec) = state
        .db
        .find_map(&ws, &id)
        .await
        .map_err(|e| db_error("load map for update", e))?
    else {
        return Err(fail(StatusCode::NOT_FOUND, "map not found"));
    };

    if let Some(name) = req.name {
        rec.name = validate_name(&name)?;
    }
    if rec.name.is_empty() {
        return Err(fail(StatusCode::BAD_REQUEST, "name must not be empty"));
    }
    if let Some(bounds) = req.bounds {
        validate_bounds(&bounds)?;
        rec.bounds = bounds;
    }
    // 省略 = 不变；显式 null = 清空；值 = 替换。
    if let Some(anchor) = req.anchor_map_id {
        rec.anchor_map_id = anchor;
    }
    validate_anchor(&state.db, &ws, rec.anchor_map_id.as_deref(), Some(&rec.id)).await?;

    if map_name_in_use(&state.db, &ws, &rec.name, Some(&rec.id)).await? {
        return Err(fail(StatusCode::CONFLICT, "map name already exists in workspace"));
    }

    match req.is_default {
        // 默认图不能被原地降级：要么指定另一张图为默认（自动提升），
        // 要么先删除。避免工作区出现"无默认图"的静默状态。
        Some(false) if rec.is_default => {
            return Err(fail(
                StatusCode::CONFLICT,
                "cannot unset the default map; promote another map instead",
            ));
        }
        Some(is_default) => rec.is_default = is_default,
        None => {}
    }

    rec.updated_at = tinyiothub_core::now_string();
    state
        .db
        .upsert_map(&rec)
        .await
        .map_err(|e| write_error("update map", e, "map name already exists in workspace"))?;
    invalidate_zones(&state, &ws);
    ok(MapView::from(rec))
}

async fn delete_map(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    WorkspaceScope(ws): WorkspaceScope,
    Path(id): Path<String>,
) -> ApiResult {
    ensure_admin(&state, &claims.user_id).await?;
    let ws = scoped_workspace(ws)?;

    let Some(map) = state
        .db
        .find_map(&ws, &id)
        .await
        .map_err(|e| db_error("load map for delete", e))?
    else {
        return Err(fail(StatusCode::NOT_FOUND, "map not found"));
    };
    if map.is_default {
        return Err(fail(
            StatusCode::CONFLICT,
            "cannot delete the default map; promote another map first",
        ));
    }
    match state.db.delete_map(&ws, &id).await {
        Ok(true) => {
            invalidate_zones(&state, &ws);
            ok(json!({"deleted": true}))
        }
        Ok(false) => Err(fail(StatusCode::NOT_FOUND, "map not found")),
        Err(e) => Err(db_error("delete map", e)),
    }
}

// ── Places ────────────────────────────────────────────

async fn ensure_map(state: &AppState, ws: &str, map_id: &str) -> Result<(), ApiError> {
    match state.db.find_map(ws, map_id).await {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(fail(StatusCode::NOT_FOUND, "map not found")),
        Err(e) => Err(db_error("load map", e)),
    }
}

async fn list_places(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Path(map_id): Path<String>,
) -> ApiResult {
    let ws = ws.unwrap_or_default();
    ensure_map(&state, &ws, &map_id).await?;
    match state.db.list_places(&ws, &map_id).await {
        Ok(places) => ok(places.into_iter().map(PlaceView::from).collect::<Vec<_>>()),
        Err(e) => Err(db_error("list places", e)),
    }
}

async fn create_place(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    WorkspaceScope(ws): WorkspaceScope,
    Path(map_id): Path<String>,
    Json(req): Json<CreatePlaceRequest>,
) -> ApiResult {
    ensure_admin(&state, &claims.user_id).await?;
    let ws = scoped_workspace(ws)?;
    ensure_map(&state, &ws, &map_id).await?;

    let name = validate_name(&req.name)?;
    validate_geometry(req.kind, &req.geometry)?;
    validate_heading(req.heading)?;

    let places = state
        .db
        .list_places(&ws, &map_id)
        .await
        .map_err(|e| db_error("list places for name check", e))?;
    if place_name_in_use(&places, &name, None) {
        return Err(fail(StatusCode::CONFLICT, "place name already exists on this map"));
    }

    let now = tinyiothub_core::now_string();
    let rec = PlaceRecord {
        id: uuid::Uuid::new_v4().to_string(),
        map_id: map_id.clone(),
        name,
        kind: req.kind,
        geometry: req.geometry,
        heading: req.heading,
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .db
        .upsert_place(&ws, &rec)
        .await
        .map_err(|e| write_error("create place", e, "place name already exists on this map"))?;
    invalidate_zones(&state, &ws);
    Ok(created(PlaceView::from(rec)))
}

async fn update_place(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    WorkspaceScope(ws): WorkspaceScope,
    Path((map_id, place_id)): Path<(String, String)>,
    Json(req): Json<UpdatePlaceRequest>,
) -> ApiResult {
    ensure_admin(&state, &claims.user_id).await?;
    let ws = scoped_workspace(ws)?;
    ensure_map(&state, &ws, &map_id).await?;

    let Some(mut rec) = state
        .db
        .find_place(&ws, &map_id, &place_id)
        .await
        .map_err(|e| db_error("load place for update", e))?
    else {
        return Err(fail(StatusCode::NOT_FOUND, "place not found"));
    };

    if let Some(name) = req.name {
        rec.name = validate_name(&name)?;
    }
    if let Some(kind) = req.kind {
        rec.kind = kind;
    }
    if let Some(geometry) = req.geometry {
        rec.geometry = geometry;
    }
    // 省略 = 不变；显式 null = 清空；值 = 替换。
    if let Some(heading) = req.heading {
        rec.heading = heading;
    }
    // kind/geometry 可能只改一项：按合并后的结果校验。
    validate_geometry(rec.kind, &rec.geometry)?;
    validate_heading(rec.heading)?;

    let places = state
        .db
        .list_places(&ws, &map_id)
        .await
        .map_err(|e| db_error("list places for name check", e))?;
    if place_name_in_use(&places, &rec.name, Some(&rec.id)) {
        return Err(fail(StatusCode::CONFLICT, "place name already exists on this map"));
    }

    rec.updated_at = tinyiothub_core::now_string();
    state
        .db
        .upsert_place(&ws, &rec)
        .await
        .map_err(|e| write_error("update place", e, "place name already exists on this map"))?;
    invalidate_zones(&state, &ws);
    ok(PlaceView::from(rec))
}

async fn delete_place(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    WorkspaceScope(ws): WorkspaceScope,
    Path((map_id, place_id)): Path<(String, String)>,
) -> ApiResult {
    ensure_admin(&state, &claims.user_id).await?;
    let ws = scoped_workspace(ws)?;

    match state.db.delete_place(&ws, &map_id, &place_id).await {
        Ok(true) => {
            invalidate_zones(&state, &ws);
            ok(json!({"deleted": true}))
        }
        Ok(false) => Err(fail(StatusCode::NOT_FOUND, "place not found")),
        Err(e) => Err(db_error("delete place", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::is_unique_violation;

    /// 预检查之外的并发重名由 SQLite UNIQUE 承担；该错误必须可被识别并
    /// 映射为 409（集成侧用并发建图验证 HTTP 状态）。
    #[tokio::test]
    async fn detects_sqlite_unique_violation() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        sqlx::query("CREATE TABLE t (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO t (id, name) VALUES ('1', 'a')")
            .execute(&pool)
            .await
            .unwrap();

        let duplicate = sqlx::query("INSERT INTO t (id, name) VALUES ('2', 'a')")
            .execute(&pool)
            .await
            .unwrap_err();
        assert!(is_unique_violation(&duplicate), "{duplicate:?}");

        let other = sqlx::query("SELECT * FROM missing_table")
            .execute(&pool)
            .await
            .unwrap_err();
        assert!(!is_unique_violation(&other), "{other:?}");
    }
}
