# API 参考

Base URL: `http://localhost:8989`（开发）或同源（生产）

所有接口返回 `{code, msg, data}`。成功 `code: 200`，业务错误 `code: 500`。

## 认证

登录后返回 JWT token，后续请求在 `Authorization: Bearer <token>` 头中携带。

未登录请求：HTTP 401 `{"code":401,"msg":"未登录"}`  
非管理员请求管理员接口：HTTP 403 `{"code":403,"msg":"需要管理员权限"}`

## 接口总览

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| POST | `/user/login` | 无 | 登录 |
| POST | `/user/register` | 无 | 注册（仅空 DB 时可用） |
| POST | `/user/changePassword` | 用户 | 修改密码 |
| GET | `/user/list` | 管理员 | 用户列表 |
| POST | `/user/create` | 管理员 | 创建用户 |
| POST | `/user/delete` | 管理员 | 删除用户 |
| GET | `/category/list` | 用户 | 分类列表 |
| POST | `/category/add` | 用户 | 新增/更新分类 |
| POST | `/category/batchUpdate` | 用户 | 批量更新排序 |
| POST | `/category/delete` | 用户 | 删除分类（级联） |
| GET | `/bookmark/list` | 用户 | 书签列表 |
| POST | `/bookmark/add` | 用户 | 新增/更新书签 |
| POST | `/bookmark/delete` | 用户 | 删除书签 |
| POST | `/bookmark/batchUpdateSort` | 用户 | 批量更新书签排序 |
| GET | `/bookmark/export` | 用户 | 导出 HTML 文件下载 |
| POST | `/bookmark/import` | 用户 | multipart 上传 HTML 文件导入 |
| POST | `/bookmark/fetchIcons` | 用户 | 抓取图标并转存为本地 base64（可指定 `ids`） |
| POST | `/bookmark/checkLinks` | 用户 | 启动链接检测（后台任务，24h 内复用缓存） |
| GET | `/bookmark/checkLinks/status` | 用户 | 轮询检测进度与结果 |

## 数据库设计

4 张表。`user` 1:N `category`，`user` 1:N `bookmark`。

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

`secrets` 存储 JWT 密钥，首次启动自动生成。`CREATE TABLE IF NOT EXISTS`，数据持久化，不再每次重启清空。

---

## 用户

### POST /user/login

**Request:**
```json
{ "username": "admin", "password": "123456" }
```

**Response (200):**
```json
{
  "code": 200,
  "msg": "操作成功",
  "data": { "id": 1, "username": "admin", "role": "admin", "token": "eyJ..." }
}
```

### POST /user/register

首次注册（user 表为空时）自动成为管理员。已有用户时返回错误。

**Request:**
```json
{ "username": "boss", "password": "pass123" }
```

**Response (200, 首次):**
```json
{ "code": 200, "msg": "操作成功", "data": null }
```

**Response (500, 已有用户):**
```json
{ "code": 500, "msg": "注册已关闭，请联系管理员添加账号", "data": null }
```

### POST /user/changePassword

需登录。验证旧密码后更新。

**Request:**
```json
{ "oldPassword": "old", "newPassword": "new123" }
```

### GET /user/list

管理员接口。返回所有用户。

**Response:**
```json
{
  "code": 200,
  "data": [
    { "id": 1, "username": "boss", "role": "admin" },
    { "id": 2, "username": "worker", "role": "user" }
  ]
}
```

### POST /user/create

管理员接口。创建普通用户。

**Request:**
```json
{ "username": "worker", "password": "pass456" }
```

### POST /user/delete

管理员接口。删除普通用户及其所有书签和分类。不能删除自己或其他管理员。

**Request:**
```json
{ "id": 2 }
```

---

## 分类

所有接口需登录，user_id 从 JWT 中提取，不再作为参数传递。

### GET /category/list

**Response:**
```json
{
  "code": 200,
  "data": [
    { "id": 1, "name": "开发工具", "userId": 1, "parentId": null, "sortOrder": 0 }
  ]
}
```

### POST /category/add

若 `id` 非空则为更新（需校验数据归属），否则为新建。

**Request:**
```json
{ "name": "新分类", "parentId": null, "sortOrder": 0 }
```

### POST /category/batchUpdate

批量更新排序（单次上限 1000 条）。仅更新属于当前用户的分类。

**Request:**
```json
[
  { "id": 1, "name": "开发工具", "parentId": null, "sortOrder": 99 }
]
```

### POST /category/delete

删除分类及其所有子孙分类，级联删除关联书签。需校验归属。

**Request:**
```json
{ "id": 1 }
```

---

## 书签

所有接口需登录，user_id 从 JWT 提取。

### GET /bookmark/list

### POST /bookmark/add

新增或更新书签。保存本身不抓取网络图标；前端保存后会异步调用 `/bookmark/fetchIcons` 补全。

**Request:**
```json
{ "title": "GitHub", "url": "https://github.com", "categoryId": 1, "icon": null }
```

### POST /bookmark/delete

```json
{ "id": 1 }
```

### GET /bookmark/export

返回 HTML 书签文件下载。图标以 `ICON="data:image/...;base64,..."` 内嵌，导入后可离线显示。

### POST /bookmark/import

multipart/form-data，仅需 `file` 字段。

| 字段 | 说明 |
|------|------|
| file | 上传的 HTML 书签文件 |

**Response:**
```json
{ "code": 200, "msg": "操作成功", "data": 5 }
```

导入行为：递归解析 `<DL><DT><H3>`（分类）和 `<DT><A>`（书签），同名同父分类复用，返回新增书签数。

### POST /bookmark/fetchIcons

服务端抓取图标并把图片字节编码为 `data:image/...;base64,...` 写入 `bookmark.icon`，之后浏览器不再请求任何外部 favicon 服务。

处理范围：`icon` 为空，或仍是 `http(s)://` 外链。单个域名在本次请求内只抓取一次。

**Request（可选，不传 `ids` 表示补齐全部）：**
```json
{ "ids": [1, 2, 3] }
```

**Response:**
```json
{ "code": 200, "data": { "total": 3, "updated": 2, "failed": 1 } }
```

抓取顺序：书签页面中的 `<link rel="icon">` / `apple-touch-icon` → `/favicon.ico` → 服务端兜底 Google / DuckDuckGo。图标大小上限 64KB，仅接受图片类型；私有/回环地址受 `ALLOW_PRIVATE_URLS` 控制。

---

## 链接检测

需登录。对所有书签逐条探测存活状态，判定分级：

| level | 含义 |
|-------|------|
| `ok` | 正常可达 |
| `suspect` | 可疑：请求被重定向到其他域名 |
| `page_dead` | 页面失效（HTTP ≥ 400），但站点根地址可达 |
| `site_dead` | 站点不可达（根地址也失败或网络错误） |

检测方式：HEAD 请求优先，失败自动回退 GET；限制 5 次重定向、单链接 3s 超时、并发 20。**24h 内重复检测直接返回缓存结果**（前端展示时带 `cached` 标记）；检测期间再次发起会返回"检测正在进行中"。进度与缓存仅存于进程内存，服务重启后失效。

### POST /bookmark/checkLinks

**Response (200, 命中缓存):**

```json
{ "code": 200, "msg": "操作成功", "data": "cached" }
```

**Response (200, 已启动后台检测):**

```json
{ "code": 200, "msg": "操作成功", "data": "ok" }
```

**Response (500, 已有任务进行中):**

```json
{ "code": 500, "msg": "检测正在进行中", "data": null }
```

### GET /bookmark/checkLinks/status

前端轮询此接口获取进度；`finished: true` 时携带 `results` 并清理任务。

**Response (进行中):**

```json
{
  "code": 200,
  "data": {
    "total": 42,
    "completed": 17,
    "finished": false,
    "results": null
  }
}
```

**Response (完成):**

```json
{
  "code": 200,
  "data": {
    "total": 42,
    "completed": 42,
    "finished": true,
    "results": [
      {
        "id": 1,
        "title": "GitHub",
        "url": "https://github.com",
        "icon": null,
        "level": "ok",
        "httpCode": 200,
        "error": null,
        "baseUrl": "https://github.com",
        "baseAlive": true,
        "finalUrl": "https://github.com/",
        "checkedAt": 1789000000,
        "cached": false
      }
    ]
  }
}
```

**Response (无任务):**

```json
{ "code": 200, "data": { "total": 0, "completed": 0, "finished": false, "results": null } }
```

前端根据结果分级分组展示，可对失效书签直接"标记正常"或点击跳转编辑。


