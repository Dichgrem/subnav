# 开发指南

## 项目概述

Subnav 是一个前后端分离的智能书签管理系统。支持分类树管理、拖拽排序、HTML 导入导出、浏览器书签兼容。

- **目标用户**：需要管理大量浏览器书签的个人/团队用户
- **核心交互**：左侧分类树 + 右侧书签网格，拖拽排序，一键导入导出
- **权限模型**：首次注册自动成为管理员，管理员创建/删除普通用户，多用户数据完全隔离

> 技术栈与项目结构见 [structure_zh-CN.md](structure_zh-CN.md)。

## 核心模块说明

### backend/src/auth.rs

基于 `jsonwebtoken` crate 实现：
- `Claims` 包含 `sub`（user_id）、`role`（admin/user）、`exp`（过期时间，默认 72h）
- `sign_token()` 签发 JWT
- `AuthUser`：从 `Authorization: Bearer <token>` 头提取 user_id，401 拦截未登录请求
- `AdminUser`：在 AuthUser 基础上校验 `role == "admin"`，403 拦截非管理员

### backend/src/handlers/user.rs

用户相关接口：
- **login**：验证 bcrypt 密码，返回 `{id, username, role, token}`
- **register**：仅当 user 表为空时允许（首次注册即管理员），之后返回"注册已关闭"
- **create_user**（管理员）：创建普通用户
- **delete_user**（管理员）：删除用户及其书签/分类，不可删自己或其他管理员
- **change_password**（已登录）：验证旧密码后更新
- **list_users**（管理员）：返回用户列表

### backend/src/handlers/category.rs

分类 CRUD + 递归删除 + 批量排序（单次 ≤1000 条）。所有操作校验数据归属（`user_id == auth.0`）。

### backend/src/handlers/bookmark.rs

书签 CRUD + 批量排序（≤1000）。HTML 导入导出的核心逻辑已拆分为独立模块：

### backend/src/handlers/export.rs / import.rs

- **export.rs**：`export_as_html()` 按分类树递归渲染 Netscape 书签 HTML，图标以内嵌 base64 导出
- **import.rs**：`import_from_html()` 行级递归解析 `<DL><DT><H3>`（分类）/ `<DT><A>`（书签），同名同父分类复用、重复 URL 跳过，返回新增书签数

### backend/src/handlers/icon.rs

图标本地化：
- `fetch_icons`：筛选 `icon` 为空或仍是 `http(s)` 外链的书签，按域名去重并发抓取（并发 8、单任务总预算 8s）
- 抓取链：页面 `<link rel="icon">` / `apple-touch-icon`（按 `sizes` 排序）→ `/favicon.ico` → 服务端兜底 Google / DuckDuckGo
- 结果统一编码为 `data:image/...;base64,...` 写入 `bookmark.icon`，浏览器之后无需再请求外部图标服务
- 抓到的栅格图标会缩放到 64×64 PNG（SVG 保持原样），避免 512×512 大图在 18px 显示时反复解码导致滚动卡顿
- 服务启动时会在后台清理历史遗留的超大 base64 图标（纯本地操作，无网络请求）
- `GET /bookmark/icon/{id}/{hash}` 把本地图标解码为原始字节返回，带一年 `immutable` 缓存；`/bookmark/list` 只返回这个短 URL，避免 base64 撑大列表
- 批量抓取时每完成一个图标立即写库，客户端断开也不会丢失已完成进度
- 安全：复用 `check.rs` 的 SSRF 校验；HTML ≤512KB、图标 ≤64KB；按魔数校验图片类型

### backend/src/handlers/check.rs

链接检测（后台任务）：
- `check_links`：登记任务并 `tokio::spawn` 后台检测；24h 内有缓存则直接复用（返回 `cached`），已有任务进行中返回"检测正在进行中"
- `check_status`：轮询进度，返回 `{total, completed, finished, results}`，完成时才携带结果并清理任务
- 内部使用 reqwest（HEAD 优先、失败 GET 回退、限 5 次重定向）+ `Semaphore` 并发 20 + 每链接 3s 超时
- 结果分级：`ok` / `suspect`（跳转跨域）/ `page_dead`（页面失效但站点可达）/ `site_dead`（站点不可达）；页面失效时会额外探测站点根地址以区分后两者
- 进度与结果保存在内存（`check_state` / `check_cache`），重启即清空

### backend/src/db.rs

初始化 SQLite 数据库：
- 使用 `rusqlite_migration` 做版本化迁移：建 `secrets` / `user` / `category` / `bookmark` 表 + 5 个索引；另有遗留 `ALTER TABLE user ADD COLUMN role` 兼容早期库
- 开启 WAL 模式与外键约束；通过 `r2d2` 连接池访问（池大小 4）
- JWT 密钥：`JWT_SECRET` 环境变量 > `secrets` 表已有值 > 首启随机生成 32 字节并持久化

## 环境要求

- [Nix](https://nixos.org/download.html)（包管理 + 隔离环境）
- Git

所有工具由 `flake.nix` 提供，无需手动安装。

```bash
nix develop     # 进入开发环境
```

**无 Nix 环境（如 Windows）**：也可直接使用本机工具链，要求 Rust（stable）、Node.js ≥ 18、pnpm（可用 `corepack enable` 启用），命令与 Nix 环境内一致。

## 常用命令

```bash
just -l              # 列出所有命令
just build           # 编译后端 + 安装前端依赖
just start-backend   # 启动后端（注入 CORS_ORIGIN）
just start-frontend  # 启动前端（localhost:5173）
just start-all       # 启动全栈
just stop            # 停止所有服务
just test            # 全部测试：cargo test + pnpm test
just fmt             # cargo fmt + clippy --fix + prettier + alejandra
just lint            # clippy + prettier --check
```

## 启动顺序

```
just start-all → 后端 :8989（注入 CORS_ORIGIN=http://localhost:5173）+ 前端 :5173
```

## 数据库设计

4 张表：

```sql
CREATE TABLE secrets (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE user (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    password TEXT NOT NULL,
    role     TEXT NOT NULL DEFAULT 'user'
);

CREATE TABLE category (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL,
    user_id    INTEGER NOT NULL,
    parent_id  INTEGER,
    sort_order INTEGER DEFAULT 0
);

CREATE TABLE bookmark (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    title       TEXT NOT NULL,
    url         TEXT NOT NULL,
    icon        TEXT,
    category_id INTEGER,
    user_id     INTEGER NOT NULL,
    sort_order  INTEGER DEFAULT 0
);
```

- `secrets` 表存储 JWT secret（`JWT_SECRET` 环境变量可覆盖），首次启动自动生成，重启不丢失
- `user.role` 为 `admin` 或 `user`，首次注册的用户即为 admin
- 5 个索引：`bookmark(user_id)` / `bookmark(category_id)` / `bookmark(url, user_id)` / `category(user_id)` / `category(parent_id)`
- 建表通过 `rusqlite_migration` 版本化迁移执行，另保留 `ALTER TABLE` 兼容旧库
- SQLite 使用 WAL 模式，外键约束开启
- 数据库文件在 `.local/bookmark.db`（可用 `DATABASE_PATH` 覆盖），数据持久化

## 架构要点

- **路由**：`auth_routes`（不需要登录：login/register）和 `api_routes`（需要 AuthUser 提取器），按功能拆分
- **状态管理**：`AppState { db, config, jwt_secret, check_state, check_cache }` 通过 `with_state` 注入
- **认证流**：Login → JWT token → localStorage → Axios interceptor 注入 `Authorization` 头 → AuthUser 提取器 → 各 handler 获取 user_id
- **错误处理**：业务错误统一经 `AppError`（实现 `From<rusqlite/r2d2/serde/JoinError>` 等转换）转为 HTTP 500 + `{code:500, msg, data:null}`；认证/鉴权失败由提取器直接返回 401/403
- **API 格式**：统一返回 `{code, msg, data}`，成功 `code: 200`，业务错误 `code: 500`，认证错误 HTTP 401/403
- **链接检测**：后台 `tokio::spawn` 任务用 reqwest 并发探测，进度放 `check_state`（Mutex），结果 24h 缓存于 `check_cache`，前端轮询 status 接口
- **端口**：默认 8989，通过 `PORT` 环境变量覆盖

## 安全

- 密码 bcrypt 哈希存储，从不明文传输
- JWT 密钥首次启动自动生成，存入数据库
- API 请求强制 Bearer Token 认证，用户数据隔离
- 管理员/普通用户角色分离，增删用户仅管理员可操作
- 零手动配置，`docker compose up` 即用

## 代码风格

- **Rust**: `cargo fmt` (rustfmt 默认配置)
- **前端**: Prettier（`pnpx prettier`）
- **Nix**: Alejandra

提交前运行 `just lint` 确保通过。测试指南见 [testing_zh-CN.md](testing_zh-CN.md)。
