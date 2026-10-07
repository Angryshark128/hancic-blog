//! REST API 扩展端点走查：post_type（page）建/列/取、settings/system 写接口、
//! stats 清零、columns 排序、attachments 删除、trails 写操作鉴权。

mod common;
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::test_app;
use hancic::db::Db;
use hancic::services::tokens;
use serde_json::{Value, json};
use tower::ServiceExt;

/// 发送 JSON 请求并返回 (状态码, JSON 响应体)。
async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let req = match body {
        Some(v) => builder.body(Body::from(v.to_string())).unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn post_type_page_create_list_and_get() {
    let (app, pool): (Router, Db) = test_app("api-ext-page").await;
    let (_tok, raw) = tokens::generate(&pool, "ci").await.unwrap();
    let token = raw.as_str();

    // 建一个 page（独立页面）+ 一个普通 post
    let (status, page_body) = send(
        &app,
        Method::POST,
        "/api/posts",
        Some(token),
        Some(json!({
            "title": "项目",
            "content_md": "# 项目",
            "status": "published",
            "post_type": "page",
            "slug": "projects"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "建 page 失败: {page_body}");
    assert_eq!(page_body["data"]["post_type"], "page");
    let page_id = page_body["data"]["id"].as_i64().unwrap();

    let (status, _) = send(
        &app,
        Method::POST,
        "/api/posts",
        Some(token),
        Some(json!({"title": "文章", "content_md": "x", "status": "published", "slug": "an-article"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 默认列表（post）不应包含 page
    let (_, list) = send(
        &app,
        Method::GET,
        "/api/posts?page_size=50",
        Some(token),
        None,
    )
    .await;
    let ids: Vec<i64> = list["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_i64().unwrap())
        .collect();
    assert!(!ids.contains(&page_id), "默认列表不应含 page: {list}");

    // type=page 应只含 page
    let (_, list) = send(
        &app,
        Method::GET,
        "/api/posts?type=page&page_size=50",
        Some(token),
        None,
    )
    .await;
    let ids: Vec<i64> = list["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_i64().unwrap())
        .collect();
    assert!(ids.contains(&page_id), "type=page 应含 page: {list}");

    // type=all 应同时含两者
    let (_, list) = send(
        &app,
        Method::GET,
        "/api/posts?type=all&page_size=50",
        Some(token),
        None,
    )
    .await;
    assert_eq!(list["data"]["total"], 2, "type=all 应含全部: {list}");

    // 按 id 取 page 详情可用
    let (status, body) = send(
        &app,
        Method::GET,
        &format!("/api/posts/{page_id}"),
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["post_type"], "page");

    // PATCH 可把 post 改成 page
    let (status, body) = send(
        &app,
        Method::PATCH,
        &format!("/api/posts/{page_id}"),
        Some(token),
        Some(json!({"post_type": "page"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "PATCH post_type 失败: {body}");
    assert_eq!(body["data"]["post_type"], "page");

    // 非法 post_type → 400
    let (status, _) = send(
        &app,
        Method::POST,
        "/api/posts",
        Some(token),
        Some(json!({"title": "x", "content_md": "y", "post_type": "bogus"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn settings_and_system_write() {
    let (app, pool): (Router, Db) = test_app("api-ext-settings").await;
    let (_tok, raw) = tokens::generate(&pool, "ci").await.unwrap();
    let token = raw.as_str();

    // 无 token → 401
    let (status, _) = send(
        &app,
        Method::PATCH,
        "/api/settings",
        None,
        Some(json!({"site_name": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 写站点名称
    let (status, body) = send(
        &app,
        Method::PATCH,
        "/api/settings",
        Some(token),
        Some(json!({"site_name": "寒蝉测试"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "写 settings 失败: {body}");
    assert_eq!(body["data"]["site_name"], "寒蝉测试");

    // 非白名单键 → 400
    let (status, _) = send(
        &app,
        Method::PATCH,
        "/api/settings",
        Some(token),
        Some(json!({"active_theme": "medium"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 非法导航 JSON → 400
    let (status, _) = send(
        &app,
        Method::PATCH,
        "/api/settings",
        Some(token),
        Some(json!({"site_nav": "not-json"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 系统设置：合法
    let (status, body) = send(
        &app,
        Method::PATCH,
        "/api/system",
        Some(token),
        Some(json!({"theme_mode": "dark", "timezone": "Asia/Shanghai"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "写 system 失败: {body}");
    assert_eq!(body["data"]["theme_mode"], "dark");

    // 系统设置：非法时区 → 400
    let (status, _) = send(
        &app,
        Method::PATCH,
        "/api/system",
        Some(token),
        Some(json!({"timezone": "Mars/Olympus"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn stats_clear_and_column_reorder() {
    let (app, pool): (Router, Db) = test_app("api-ext-clear").await;
    let (_tok, raw) = tokens::generate(&pool, "ci").await.unwrap();
    let token = raw.as_str();

    // 清空统计（无 token 401）
    let (status, _) = send(&app, Method::POST, "/api/stats/clear", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, body) = send(&app, Method::POST, "/api/stats/clear", Some(token), None).await;
    assert_eq!(status, StatusCode::OK, "清空统计失败: {body}");
    assert_eq!(body["data"]["ok"], true);

    // 建两个专栏后排序
    let (_, c1) = send(
        &app,
        Method::POST,
        "/api/columns",
        Some(token),
        Some(json!({"name": "甲"})),
    )
    .await;
    let (_, c2) = send(
        &app,
        Method::POST,
        "/api/columns",
        Some(token),
        Some(json!({"name": "乙"})),
    )
    .await;
    let a = c1["data"]["id"].as_i64().unwrap();
    let b = c2["data"]["id"].as_i64().unwrap();
    let (status, body) = send(
        &app,
        Method::POST,
        "/api/columns/reorder",
        Some(token),
        Some(json!({"ids": [b, a]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "专栏排序失败: {body}");
    // 空 ids → 400
    let (status, _) = send(
        &app,
        Method::POST,
        "/api/columns/reorder",
        Some(token),
        Some(json!({"ids": []})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn attachments_delete_and_trails_write_auth() {
    let (app, pool): (Router, Db) = test_app("api-ext-del").await;
    let (_tok, raw) = tokens::generate(&pool, "ci").await.unwrap();
    let token = raw.as_str();

    // 插入一条附件记录（磁盘文件不存在 → 删除仍应成功并清行）
    sqlx::query(
        "INSERT INTO attachments(uuid_name, orig_name, mime, size, kind, path)
         VALUES ('u1', 'a.png', 'image/png', 10, 'image', 'images/u1.png')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (status, _) = send(&app, Method::DELETE, "/api/attachments/1", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(
        &app,
        Method::DELETE,
        "/api/attachments/1",
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row: Option<i64> = sqlx::query_scalar("SELECT id FROM attachments WHERE id = 1")
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(row.is_none(), "附件行应已删除");

    // 轨迹写操作需鉴权
    let (status, _) = send(&app, Method::DELETE, "/api/trails/1", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // 不存在的轨迹删除 → 204（delete_trail 对不存在按成功处理）
    let (status, _) = send(&app, Method::DELETE, "/api/trails/999", Some(token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}
