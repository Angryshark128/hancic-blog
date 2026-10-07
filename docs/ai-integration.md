# Hancic REST API 集成指南（AI 专用）

本文档是给 AI 智能体（及未来 MCP server）对接 hancic 博客的接口手册。
所有端点基于 HTTP + JSON，鉴权用 API Token（Bearer）。

## 1. 概览

- **Base URL**：`https://<你的域名>/api`（本地开发 `http://127.0.0.1:8080/api`，端口以实际配置为准）
- **响应格式**：成功统一 `{"data": ...}`；失败统一 `{"error": {"code": <HTTP状态码>, "message": "<中文说明>"}}`
- **鉴权**：除 `/api/health` 外全部端点要求 `Authorization: Bearer <token>`（后台管理员会话也可，AI 一律用 Bearer）
- **时间**：所有时间字段为 UTC ISO 8601（如 `2026-08-09T02:00:00.000000000Z`）；`from`/`to` 类查询参数用 `YYYY-MM-DD`

### 状态码约定

| 状态码 | 含义 |
|---|---|
| 200 | 成功 |
| 201 | 创建成功（POST 类） |
| 204 | 删除成功（无响应体） |
| 400 | 请求体/参数校验失败（空标题、非法 status、分类不存在等） |
| 401 | 未携带或 Token 无效 |
| 404 | 资源不存在 |
| 409 | 冲突（如分类 slug 重复） |
| 500 | 服务端错误 |

## 2. 鉴权

1. 登录后台 `/admin/tokens` 生成 API Token，明文形如 `hc_xxxxxxxx...`，**仅显示一次**，请立即保存。
2. 请求头携带：`Authorization: Bearer hc_xxxxxxxx...`

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  https://example.com/api/posts
```

Token 无效或缺失返回：

```json
{"error": {"code": 401, "message": "无效的 API Token"}}
```

**错误处理建议**：收到 401 时检查 Token 是否已复制完整（46 字符，`hc_` 开头）或已在后台吊销；吊销后需重新生成。

## 3. 文章（Posts）

### 3.1 创建文章

```bash
curl -s -X POST https://example.com/api/posts \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{
    "title": "我的第一篇 API 文章",
    "content_md": "# 标题\n\n正文用 **Markdown** 书写，原文会被完整保存。",
    "slug": "my-first-api-post",
    "excerpt": "正文用 Markdown 书写，原文会被完整保存。",
    "status": "draft",
    "category_id": 1,
    "tags": ["ai", "指南"]
  }'
```

响应 `201 Created`：

```json
{
  "data": {
    "id": 42,
    "slug": "my-first-api-post",
    "title": "我的第一篇 API 文章",
    "content_md": "# 标题\n\n正文用 **Markdown** 书写，原文会被完整保存。",
    "excerpt": "正文用 Markdown 书写，原文会被完整保存。",
    "status": "draft",
    "post_type": "post",
    "published_at": null,
    "created_at": "2026-08-09T02:00:00.000000000Z",
    "updated_at": "2026-08-09T02:00:00.000000000Z",
    "views": 0,
    "category_id": 1
  }
}
```

请求体字段：

| 字段 | 必填 | 说明 |
|---|---|---|
| `title` | ✅ | 非空字符串 |
| `content_md` | ✅ | Markdown 原文（可为空串，用于先建草稿） |
| `slug` | ❌ | 缺省由标题自动生成；重复时自动追加 `-2`、`-3`… |
| `excerpt` | ❌ | 摘要；缺省自动截取正文 |
| `status` | ❌ | `draft`（默认）或 `published`；发布即写 `published_at` |
| `post_type` | ❌ | `post`（默认，普通文章）或 `page`（独立页面，如「关于」「项目」） |
| `category_id` | ❌ | 必须指向存在的分类，否则 400 |
| `tags` | ❌ | 字符串数组，自动建标签（同名复用） |

### 3.2 列表与筛选

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://example.com/api/posts?page=1&page_size=10&status=published&category=tech&tag=ai&type=post"
```

- `page`：从 1 起，默认 1；`page_size`：默认 10，上限 100
- `status`：`draft` / `published`；`category` / `tag`：slug 精确匹配
- `type`：`post`（缺省，普通文章）/ `page`（独立页面）/ `all`（全部）。**独立页面默认不在列表里**，需显式 `type=page` 或 `type=all` 才会出现。

响应 `{data: {items: [Post...], total: N}}`，Post 结构与 3.1 相同（含 `content_md` 原文与 `views`）。

### 3.3 文章详情

```bash
curl -s -H "Authorization: Bearer $TOKEN" https://example.com/api/posts/42
```

不存在返回 `404 {"error": {"code": 404, "message": "文章不存在"}}`。

### 3.4 更新文章（PATCH，部分字段）

```bash
curl -s -X PATCH https://example.com/api/posts/42 \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"title": "改过的标题", "status": "published", "category_id": null}'
```

PATCH 语义：

- 缺失字段**保持不变**
- `"category_id": null` → **清空分类**；传整数则改分类（需存在）
- `"excerpt": ""` 或 `"excerpt": null` → **清空摘要**
- `"post_type": "page"` → 改类型为独立页面（或 `"post"` 改回普通文章）
- `"status": "published"` 会写入 `published_at`（草稿 → 发布时）
- `"published_at"`：可选 RFC3339 字符串 → 设值；`null` → 清空（回到草稿合法值）；缺省 → 不变。**显式值覆盖**「草稿 → 发布」自动设值。
- `"updated_at"`：可选 RFC3339 字符串 → 设值。**传了就跳过自动刷 `Utc::now()`**，允许事后回填到非工作时间窗口。非法格式 → 400。

### 3.5 事后回填时间戳（专用白名单端点）

仅修改 `published_at` 与 `updated_at`，不动内容/分类/标签/状态——审计/排查时一眼能看出「这次只动了时间」。用于工作时间窗口外的事后回填（路线 v1.10：所有 GitHub/外部时间戳必须落在工作日 08:00–09:30 / 22:30–01:30 或任意周末）。

```bash
curl -s -X POST https://example.com/api/posts/42/timestamps \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{
    "published_at": "2026-09-15T01:30:00Z",
    "updated_at":  "2026-09-15T01:30:00Z"
  }'
```

请求体字段：

| 字段 | 必填 | 说明 |
|---|---|---|
| `published_at` | 二选一 | RFC3339 字符串 → 设值；`null` → 清空（合法：回到草稿状态时 published_at 为 NULL） |
| `updated_at` | 二选一 | RFC3339 字符串 → 设值；**不允许 null**（updated_at 必非空） |

至少传一个字段，否则 400。响应 `200 OK`，返回完整 Post 对象（与 `GET /api/posts/{id}` 形态一致）。

### 3.6 删除文章

```bash
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" https://example.com/api/posts/42
```

成功 `204`（无响应体）；不存在 `404`。

## 4. 说说（Moments）

### 4.1 创建

```bash
curl -s -X POST https://example.com/api/moments \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"content": "今天的碎碎念…", "attachment_ids": [3, 5]}'
```

`content` 必填非空；`attachment_ids` 可选整数数组（需指向已上传的附件，否则 400）。响应 `201 {data: {id, content, created_at}}`。

### 4.2 删除

```bash
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" https://example.com/api/moments/7
```

成功 `204`；不存在 `404`。

### 4.3 列表 / 详情 / 更新

```bash
# 列表（page/page_size 分页；q 关键词；month=YYYY-MM；order=asc|desc）
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://example.com/api/moments?page=1&page_size=10&q=碎碎念"
# 详情（含 attachments 数组）
curl -s -H "Authorization: Bearer $TOKEN" https://example.com/api/moments/7
# 更新（部分更新：content 与 attachment_ids 只更提供的字段；[] 清空附件）
curl -s -X PATCH https://example.com/api/moments/7 \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"content": "改后的正文", "attachment_ids": [8]}'
```

列表响应 `{data: {items: [{id, content, created_at, like_count, attachments}], total}}`。
`attachments` 数组元素含 `id/kind/orig_name/mime/url`（url 为前台静态地址）。

## 5. 附件上传

```bash
curl -s -X POST https://example.com/api/uploads \
  -H "Authorization: Bearer $TOKEN" \
  -F "files=@/path/to/image.png"
```

- multipart 字段名固定为 `files`，可一次传多个文件
- 类型白名单：图片（jpeg/png/webp/gif）、视频（mp4/webm/mov）、文件（pdf/txt/zip/gz/bin/md）；超限或类型不符 400
- 响应 `{data: [Attachment...]}`，`Attachment` 含 `id`（后续写文章/说说时引用）、`url 相关 path` 等字段

### 附件库列表

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://example.com/api/attachments?kind=image&q=截图&order=desc&page=1&page_size=20"
```

- `kind`：image/video/file（缺省全部）；`q` 文件名关键词；`order` asc/desc
- 响应 `{data: {items: [{id, kind, orig_name, mime, size, url, created_at}], total, page, page_size}}`；
  `url` 为前台公开地址，可直接用于文章正文 / 说说 `attachment_ids` 前的选取

### 删除附件

```bash
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" https://example.com/api/attachments/7
```

删除磁盘文件与数据库记录（成功 `204`）。

## 6. 分类（Categories）

```bash
# 列表
curl -s -H "Authorization: Bearer $TOKEN" https://example.com/api/categories
# 创建
curl -s -X POST https://example.com/api/categories \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"name": "技术", "slug": "tech", "sort_order": 1}'
# 更新（PATCH 部分字段）
curl -s -X PATCH https://example.com/api/categories/1 \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"name": "工程技术"}'
# 删除
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" https://example.com/api/categories/1
```

- 创建必填 `name`；`slug` 缺省由名称生成；重复 slug → 409
- 删除分类时关联文章的 `category_id` 自动置空（文章不删）

## 7. 标签（Tags）

```bash
# 列表（含各标签已发布文章数）
curl -s -H "Authorization: Bearer $TOKEN" https://example.com/api/tags
# 删除（关联文章不受影响，级联清理文章-标签关联）
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" https://example.com/api/tags/1
```

- 创建为幂等语义：同名（按 slug）返回既有记录，不重复建标签（名称 ≤5 字，超长 400）
- 删除不可恢复；常用于清理无文章的残留标签

```bash
# 创建（幂等）
curl -s -X POST https://example.com/api/tags \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"name": "AI"}'
# 响应 {data: {id, slug, name}}
```

## 8. 专栏（Columns）

```bash
# 列表（含各专栏已发布文章数）
curl -s -H "Authorization: Bearer $TOKEN" https://example.com/api/columns
# 创建（name 必填 ≤8 字；description ≤50 字；slug 缺省由名称自动生成，中文原样保留）
curl -s -X POST https://example.com/api/columns \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"name": "工程思维", "description": "把工程思维用到生活和决策里"}'
# 更新名称/描述（PATCH 部分字段；slug 创建后不改，避免前台链接失效）
curl -s -X PATCH https://example.com/api/columns/4 \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"description": "新的专栏描述"}'
# 删除（关联文章自动变为无专栏，文章不删）
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" https://example.com/api/columns/4

# 专栏下文章列表（分页，仅已发布，按更新时间倒序）
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://example.com/api/columns/4/posts?page=1&page_size=10"
# 把文章加入专栏
curl -s -X POST https://example.com/api/columns/4/posts \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"post_id": 42}'
# 把文章移出专栏（文章保留）
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" \
  https://example.com/api/columns/4/posts/42

# 专栏排序（按 ids 顺序重写 sort_order）
curl -s -X POST https://example.com/api/columns/reorder \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"ids": [4, 2, 7]}'
# 专栏内文章排序（按 ids 顺序重写 column_sort）
curl -s -X POST https://example.com/api/columns/4/posts/reorder \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"ids": [42, 51, 33]}'
```

- 一篇文章可属于 0/1 个专栏（`posts.column_id`）；加入/移出不触碰文章其他字段
- 校验与后台一致：名称超 8 字、描述超 50 字 → 400；slug 冲突 → 409

## 9. 统计

```bash
# 阅读汇总（总量 + 每日趋势）
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://example.com/api/stats/summary?from=2026-08-01&to=2026-08-09"
# 清空阅读明细日志（不可恢复）
curl -s -X POST -H "Authorization: Bearer $TOKEN" https://example.com/api/stats/clear
```

响应 `{data: {total_views, total_posts, total_moments, total_attachments, trend: [{date, count}...]}}`。`from`/`to` 可选（站点时区日期）。

## 10. 站点设置 / 系统设置 / 主题 / 轨迹

### 10.1 站点设置

```bash
# 读取全部设置键值（站点名/描述/Logo/导航/社交/页脚文本等）
curl -s -H "Authorization: Bearer $TOKEN" https://example.com/api/settings
# 局部更新（只传要改的键，未传保持不变；返回更新后全量设置）
curl -s -X PATCH https://example.com/api/settings \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"site_name": "寒蝉", "site_desc": "一个人的独白"}'
```

- 可写白名单键：`site_name` / `site_desc` / `site_nav` / `site_social` / `social_logos` /
  `site_logo` / `footer_text` / `friend_links` / `contact_enabled` / `contact_email`；
  非白名单键被忽略，全部被忽略时 400。
- `site_nav` / `site_social` / `social_logos` / `friend_links` 为 JSON 字符串；
  校验规则与后台设置页一致（非法 JSON → 400）。

### 10.2 系统设置

```bash
# 读取（theme_mode / timezone / date_format）
curl -s -H "Authorization: Bearer $TOKEN" https://example.com/api/system
# 局部更新
curl -s -X PATCH https://example.com/api/system \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"theme_mode": "dark", "timezone": "Asia/Shanghai"}'
```

`theme_mode`：`auto`|`light`|`dark`；`timezone`：IANA 名（如 `Asia/Shanghai`）；
`date_format`：`datetime`|`date`。改管理员密码仍走后台。

### 10.3 主题

```bash
# 列表（含 is_current）与当前主题
curl -s -H "Authorization: Bearer $TOKEN" https://example.com/api/themes
# 安装（上传主题 zip，multipart 字段名 theme）
curl -s -X POST https://example.com/api/themes/import \
  -H "Authorization: Bearer $TOKEN" -F "theme=@/path/to/theme.zip"
# 切换（同名目录 + theme.toml 校验；404=不存在）
curl -s -X POST -H "Authorization: Bearer $TOKEN" https://example.com/api/themes/default/activate
# 卸载（不许卸载当前使用中的主题）
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" https://example.com/api/themes/old-theme
```

切换写入 `settings.active_theme`，前台模板需重启服务后完全生效。

### 10.4 徒步轨迹

```bash
# 列表（统计概览）
curl -s -H "Authorization: Bearer $TOKEN" https://example.com/api/trails
# 导入 GPX（multipart 字段名 files；可多次 -F "files=@..."）
curl -s -X POST https://example.com/api/trails/import \
  -H "Authorization: Bearer $TOKEN" -F "files=@/path/to/track.gpx"
# 详情；加 with_coords=1 附完整坐标 [[lat, lon, speed], ...]
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://example.com/api/trails/3?with_coords=1"
# 更新名称/描述（PATCH 部分字段）
curl -s -X PATCH https://example.com/api/trails/3 \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"name": "香山环线", "description": "秋天的香山"}'
# 删除（清理 GPX + 坐标 JSON）
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" https://example.com/api/trails/3
```

导入返回 `{data: {imported: [...], imported_count, errors: [...]}}`（逐个导入，失败不中断其余）。

## 11. 健康检查

```bash
curl -s https://example.com/api/health
# {"data": {"status": "ok"}}
```

不鉴权，供监控/部署探测。

## 12. 发布文章最佳实践（给 Agent 的推荐流程）

1. **先查再写**：`GET /api/categories` 确认分类存在（或先 `POST /api/categories` 建分类）；标签同名自动复用，无需预建。
2. **图片先传**：文章里的图片先 `POST /api/uploads` 拿到 `id`/URL，再写进 `content_md`。
3. **先草稿后发布**：长文建议先 `status: "draft"` 创建拿到 `id`，分多次 `PATCH` 完善，最后 `PATCH {"status": "published"}` 发布——避免半成品直接上线。
4. **校验返回**：每次写操作检查响应中的 `data.id`（201/200）确认成功；失败按 `error.code` 分类处理：
   - `400`：按 `message` 修正请求体；
   - `401`：Token 问题（见第 2 节）；
   - `409`：slug 冲突，换 slug 或去掉让服务端生成；
   - `5xx`：服务端异常，稍后重试并保留请求体。
5. **幂等注意**：POST 无幂等键，重复提交会重复建文章；如需保证只建一次，先 `GET /api/posts?page=1&page_size=1&status=draft` 核对或事后清理。

## 13. MCP 封装说明

MCP server（`mcp/hancic_mcp.py`，及集中托管的 `hancic-mcp-server`）已基于本文档实现，
当前共 **47 个工具**，与上文的 `/api/*` 端点一一对应：

- 文章：`list_posts`（`post_type=post|page|all`）/ `get_post` / `create_post` /
  `update_post` / `delete_post` / `set_post_timestamps`
- 说说：`list_moments` / `get_moment` / `create_moment` / `update_moment` / `delete_moment`
- 分类/标签：`list_categories` / `create_category` / `update_category` / `delete_category` /
  `list_tags` / `create_tag` / `delete_tag`
- 专栏：`list_columns` / `create_column` / `update_column` / `delete_column` /
  `list_column_posts` / `add_post_to_column` / `remove_post_from_column` /
  `reorder_columns` / `reorder_column_posts`
- 附件：`list_attachments` / `upload_attachment` / `delete_attachment`
- 站点/系统设置：`get_settings` / `update_settings` / `get_system_settings` / `update_system_settings`
- 主题：`list_themes` / `activate_theme` / `import_theme` / `uninstall_theme`
- 轨迹：`list_trails` / `get_trail` / `import_trails` / `update_trail` / `delete_trail`
- 统计/备份/健康：`get_stats` / `clear_stats` / `get_backup` / `get_health`

映射约定：

- 鉴权：Token 存于 MCP server 环境变量（`HANCIC_API_TOKEN`），所有请求统一注入
  `Authorization` 头，不暴露给调用方
- 入参校验在 tool 层做（title 非空、status 枚举、category_id 存在性），把 400 提前转成
  tool 参数错误，减少对服务端的无效请求
- 高危运维操作（备份恢复、站点迁移、改管理员密码、API Token 管理）仍走后台，不暴露为工具
- 发布文章最佳实践（第 12 节）可由调用方组合步骤，或后续固化为 `publish_article` 组合工具
