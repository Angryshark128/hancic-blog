//! REST API 主题管理：GET /api/themes（列表 + 当前）、POST /api/themes/{name}/activate、
//! POST /api/themes/import（上传 zip 安装）、DELETE /api/themes/{name}（卸载）。
//!
//! 主题以 `data/themes/{name}` 目录存在（theme.toml 声明元信息）；「当前主题」
//! 存于 `settings.active_theme`（缺省回落配置默认值）。切换写入后前台模板
//! 需重启服务完全生效（与后台页面一致的口径）。

use crate::api;
use crate::error::AppError;
use crate::services::settings;
use crate::{AppState, themes};
use axum::Json;
use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use tower_sessions::Session;

/// 主题 zip 上传上限（100MB，与后台一致）。
const MAX_THEME_ZIP_BYTES: usize = 100 * 1024 * 1024;

/// GET /api/themes：已安装主题列表（含当前主题）。
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    api::require_admin_or_token(&state, &session, &headers).await?;
    let themes_dir = state.config.data_dir.join("themes");
    let metas = themes::discover(&themes_dir).unwrap_or_default();
    let current = settings::get(&state.db, "active_theme")
        .await?
        .filter(|s| !s.is_empty())
        .unwrap_or(state.config.active_theme.clone());
    let items: Vec<Value> = metas
        .iter()
        .map(|m| {
            json!({
                "name": m.name,
                "author": m.author,
                "version": m.version,
                "description": m.description,
                "is_current": m.name == current,
            })
        })
        .collect();
    Ok(Json(
        json!({ "data": { "items": items, "current": current } }),
    ))
}

/// POST /api/themes/{name}/activate：把主题写为当前主题（需重启完全生效）。
pub async fn activate(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    api::require_admin_or_token(&state, &session, &headers).await?;
    // 主题必须真实存在（目录 + theme.toml），防止写入不存在的名字
    let themes_dir = state.config.data_dir.join("themes");
    if !themes::is_valid_name(&name) || themes::load_meta(&themes_dir, &name).is_err() {
        return Err(AppError::NotFound(format!("主题不存在: {name}")));
    }
    settings::set(&state.db, "active_theme", &name).await?;
    Ok(Json(json!({
        "data": {
            "name": name,
            "note": "已切换，重启服务后前台完全生效"
        }
    })))
}

/// POST /api/themes/import：multipart 上传主题 zip（字段名 `theme`）安装。
pub async fn import(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    api::require_admin_or_token(&state, &session, &headers).await?;
    let mut zip_bytes: Option<Vec<u8>> = None;
    let mut parts = multipart;
    while let Some(field) = parts
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("解析上传内容失败: {e}")))?
    {
        if field.name() == Some("theme") {
            zip_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("读取主题包失败: {e}")))?
                    .to_vec(),
            );
        }
    }
    let Some(bytes) = zip_bytes else {
        return Err(AppError::BadRequest(
            "未提供主题 zip（字段名 theme）".into(),
        ));
    };
    if bytes.is_empty() {
        return Err(AppError::BadRequest("主题包为空".into()));
    }
    if bytes.len() > MAX_THEME_ZIP_BYTES {
        return Err(AppError::BadRequest("主题包超过 100MB 上限".into()));
    }
    let themes_dir = state.config.data_dir.join("themes");
    match themes::install(&themes_dir, &bytes) {
        Ok(meta) => {
            tracing::info!("主题导入成功: {} v{}", meta.name, meta.version);
            state.theme_cache.invalidate(&meta.name).await;
            Ok(Json(json!({
                "data": {
                    "name": meta.name,
                    "version": meta.version,
                    "note": "已安装，重启服务后可用"
                }
            })))
        }
        Err(e) => Err(AppError::BadRequest(format!("导入失败: {e}"))),
    }
}

/// DELETE /api/themes/{name}：卸载主题（不许卸载当前使用中的主题，成功 204）。
pub async fn uninstall(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<StatusCode, AppError> {
    api::require_admin_or_token(&state, &session, &headers).await?;
    let themes_dir = state.config.data_dir.join("themes");
    if !themes::is_valid_name(&name) || themes::load_meta(&themes_dir, &name).is_err() {
        return Err(AppError::NotFound(format!("主题不存在: {name}")));
    }
    let current = settings::get(&state.db, "active_theme")
        .await?
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| state.config.active_theme.clone());
    if current == name {
        return Err(AppError::BadRequest("不能卸载当前使用的主题".into()));
    }
    std::fs::remove_dir_all(themes_dir.join(&name))
        .map_err(|e| AppError::Internal(format!("卸载失败: {e}")))?;
    tracing::info!("主题卸载: {name}");
    state.theme_cache.invalidate(&name).await;
    Ok(StatusCode::NO_CONTENT)
}
