//! REST API 徒步轨迹：GET /api/trails（列表）、GET /api/trails/{id}（详情）、
//! POST /api/trails/import（上传 GPX 导入）、PATCH /api/trails/{id}（改名/描述）、
//! DELETE /api/trails/{id}（删除，清理磁盘 GPX 与坐标 JSON）。
//!
//! 轨迹数据展示字段含里程/爬升/时长；详情额外可选完整坐标（`?with_coords=1`）。

use crate::AppState;
use crate::api;
use crate::error::AppError;
use crate::models::Trail;
use crate::services::trails;
use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::collections::HashMap;
use tower_sessions::Session;

/// 轨迹 JSON：基础字段 + 展示用格式化字段（url 为前台详情页路由）。
fn trail_json(t: &Trail) -> Value {
    json!({
        "id": t.id,
        "name": t.name,
        "description": t.description,
        "url": format!("/trails/{}", t.id),
        "started_at": t.started_at.map(|d| d.date_naive().to_string()),
        "distance_m": t.distance_m,
        "distance_km_str": t.distance_m.map(|m| format!("{:.1}", m / 1000.0)),
        "elevation_gain_m": t.elevation_gain_m,
        "elevation_gain_str": t.elevation_gain_m.map(|e| format!("{e:.0}")),
        "elevation_loss_m": t.elevation_loss_m,
        "moving_seconds": t.moving_seconds,
        "moving_time": trails::format_moving(t.moving_seconds),
        "avg_speed_kmh": t.avg_speed_kmh,
        "avg_speed_str": t.avg_speed_kmh.map(|s| format!("{s:.1}")),
        "max_elevation_m": t.max_elevation_m,
        "min_elevation_m": t.min_elevation_m,
        "point_count": t.point_count,
    })
}

/// GET /api/trails：轨迹列表（按开始时间/创建序，最近优先）。
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    api::require_admin_or_token(&state, &session, &headers).await?;
    let items = trails::list_trails(&state.db, trails::TrailSort::parse(None)).await?;
    Ok(Json(json!({
        "data": { "items": items.iter().map(trail_json).collect::<Vec<_>>(), "total": items.len() }
    })))
}

/// GET /api/trails/{id}：轨迹详情；`?with_coords=1` 附加完整坐标数组
/// `[[lat, lon, speed?], ...]`（speed 单位 km/h，可能为 null）。
pub async fn get(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    api::require_admin_or_token(&state, &session, &headers).await?;
    let trail = trails::get_trail(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("轨迹不存在".into()))?;
    let mut data = trail_json(&trail);
    if query.get("with_coords").is_some_and(|v| v == "1") {
        let dir = state.config.data_dir.join("trails");
        let coords = trails::load_full_coords(&dir, id)
            .unwrap_or_default()
            .into_iter()
            .map(|(lat, lon, spd)| json!([lat, lon, serde_json::Value::from(spd)]))
            .collect::<Vec<_>>();
        data["coords"] = json!(coords);
    }
    Ok(Json(json!({ "data": data })))
}

/// POST /api/trails/import：multipart 上传 GPX（字段名 `files`，可多选）。
/// 可选表单字段 `name`/`description` 覆盖默认名称与描述（为空则用 GPX 文件名）。
/// 逐个导入，汇总成功/失败，返回每条结果。
pub async fn import(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    api::require_admin_or_token(&state, &session, &headers).await?;
    let mut name = String::new();
    let mut description = String::new();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut parts = multipart;
    while let Some(field) = parts
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("解析上传内容失败: {e}")))?
    {
        match field.name() {
            Some("name") => name = field.text().await.unwrap_or_default(),
            Some("description") => description = field.text().await.unwrap_or_default(),
            Some("files") => {
                let file_name = field.file_name().map(str::to_string).unwrap_or_default();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("读取上传文件失败: {e}")))?;
                files.push((file_name, data.to_vec()));
            }
            _ => {}
        }
    }
    if files.is_empty() {
        return Err(AppError::BadRequest(
            "未提供 GPX 文件（字段名 files）".into(),
        ));
    }
    let trails_dir = state.config.data_dir.join("trails");
    let mut imported: Vec<Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for (file_name, data) in files {
        if !file_name.to_lowercase().ends_with(".gpx") {
            errors.push(format!("{file_name}：仅支持 .gpx 文件"));
            continue;
        }
        if data.is_empty() {
            errors.push(format!("{file_name}：文件内容为空"));
            continue;
        }
        let fallback_name = file_name
            .strip_suffix(".gpx")
            .or_else(|| file_name.strip_suffix(".GPX"))
            .unwrap_or(&file_name)
            .to_string();
        match trails::import_gpx(
            &state.db,
            &trails_dir,
            &name,
            &fallback_name,
            &description,
            &data,
        )
        .await
        {
            Ok(t) => imported.push(trail_json(&t)),
            Err(e) => errors.push(format!("{file_name}：{}", e.message())),
        }
    }
    Ok(Json(json!({
        "data": { "imported": imported, "imported_count": imported.len(), "errors": errors }
    })))
}

/// PATCH /api/trails/{id}：更新轨迹名称/描述（不存在 404）。
pub async fn update(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    Path(id): Path<i64>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AppError> {
    api::require_admin_or_token(&state, &session, &headers).await?;
    let body = api::valid_json(body)?;
    let trail = trails::get_trail(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("轨迹不存在".into()))?;
    let name = match body.get("name").and_then(Value::as_str) {
        Some(s) => s.trim().to_string(),
        None => trail.name.clone(),
    };
    if name.is_empty() {
        return Err(AppError::BadRequest("轨迹名称不能为空".into()));
    }
    if name.chars().count() > 100 {
        return Err(AppError::BadRequest("轨迹名称最多 100 字".into()));
    }
    let description = match body.get("description").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => trail.description.clone(),
    };
    if description.chars().count() > 500 {
        return Err(AppError::BadRequest("轨迹描述最多 500 字".into()));
    }
    let updated = trails::update_trail(&state.db, id, &name, &description).await?;
    Ok(Json(json!({ "data": trail_json(&updated) })))
}

/// DELETE /api/trails/{id}：删除轨迹（磁盘 GPX + 坐标 JSON + 记录，成功 204）。
pub async fn delete(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    api::require_admin_or_token(&state, &session, &headers).await?;
    let trails_dir = state.config.data_dir.join("trails");
    trails::delete_trail(&state.db, &trails_dir, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
