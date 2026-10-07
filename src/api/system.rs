//! REST API 系统设置：GET /api/system、PATCH /api/system。
//!
//! 系统级设置（主题模式 / 时区 / 日期格式）的白名单读写，复用后台「系统设置」页
//! 的校验逻辑。改管理员密码属于高敏操作，仍走后台页面，不在此暴露。

use crate::AppState;
use crate::api;
use crate::error::AppError;
use crate::services::settings;
use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::HeaderMap;
use serde_json::{Value, json};
use std::collections::HashMap;
use tower_sessions::Session;

/// 可写的系统设置键（与后台系统设置页表单一致）。
const WRITABLE_KEYS: [&str; 3] = ["theme_mode", "timezone", "date_format"];

/// GET /api/system：当前系统设置（白名单键值）。
pub async fn get(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    api::require_admin_or_token(&state, &session, &headers).await?;
    let map = settings::get_many(&state.db, &WRITABLE_KEYS).await?;
    Ok(Json(json!({ "data": map })))
}

/// PATCH /api/system：局部更新白名单内的键（缺失键不变）。
pub async fn update(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AppError> {
    api::require_admin_or_token(&state, &session, &headers).await?;
    let body = api::valid_json(body)?;
    let obj = body
        .as_object()
        .ok_or_else(|| AppError::BadRequest("请求体必须是 JSON 对象".into()))?;
    let mut form: HashMap<String, String> = HashMap::new();
    for key in WRITABLE_KEYS {
        if let Some(v) = obj.get(key) {
            let s = match v {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            };
            form.insert(key.to_string(), s.trim().to_string());
        }
    }
    if form.is_empty() {
        return Err(AppError::BadRequest(format!(
            "没有可更新的字段；可写键：{}",
            WRITABLE_KEYS.join(", ")
        )));
    }
    let errors = crate::admin::system::validate(&form);
    if !errors.is_empty() {
        return Err(AppError::BadRequest(errors.join("；")));
    }
    for (key, value) in &form {
        settings::set(&state.db, key, value).await?;
    }
    let map = settings::get_many(&state.db, &WRITABLE_KEYS).await?;
    Ok(Json(json!({ "data": map })))
}
