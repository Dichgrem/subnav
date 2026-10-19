# Subnav

前后端分离的书签管理工具，支持分类树管理、拖拽排序、HTML 导入导出，兼容浏览器书签格式。

## 功能

- **用户系统**：首次启动自动创建管理员（默认 admin/password），管理员可创建/删除用户，多用户数据隔离
- **JWT 认证**：自动签发和验证，无需手动配置密钥
- **分类树管理**：新建、重命名、删除、拖拽排序
- **书签管理**：新建、编辑、删除、拖拽排序、分类间移动
- **HTML 导入导出**：兼容 Chrome / Firefox / Edge 书签格式
- **书签图标自动抓取并本地存储**（base64 存库 + 独立图标接口长期缓存，进站不再请求 Google/DuckDuckGo）
- **模糊搜索**：标题/网址模糊匹配，支持拼音（Fuse.js + pinyin-pro）
- **链接检测**：批量探测书签存活状态，区分正常 / 页面失效 / 站点不可达 / 可疑跳转
- **修改密码**
- **中英文切换、亮色/暗色模式**
- **吃豆人加载动画**

## 技术栈

| 层 | 技术 |
|---|---|
| 后端 | Rust, Axum 0.7, SQLite (rusqlite + r2d2 连接池 + rusqlite_migration), JWT (jsonwebtoken), reqwest (链接检测) |
| 前端 | Vue 3, Element Plus (按需引入), Vite, vue-i18n, Fuse.js + pinyin-pro (模糊搜索) |
| 开发环境 | Nix, just |
| 部署 | Docker (musl 静态编译, FROM scratch, ~15MB) |

## 快速开始

### Docker（推荐）

```bash
docker compose up --build -d
# 打开 http://localhost:8989
# 默认账号：admin / password（可通过 ADMIN_USERNAME / ADMIN_PASSWORD 环境变量自定义）
```

### 开发环境

```bash
nix develop
just build
just start-all
# 前端 http://localhost:5173 后端 http://localhost:8989
```

## 常用命令

```bash
just -l              # 列出所有命令
just build           # 编译后端 + 安装前端依赖
just start-all       # 启动全栈
just stop            # 停止所有服务
just fmt             # 格式化 + 自动修复（fmt + clippy --fix + prettier + alejandra）
just lint            # 只读检查（clippy + prettier --check）
just test            # 运行全部测试（cargo test + pnpm test）
```

## 文档

| 文档 | 内容 |
|------|------|
| [项目结构](docs/structure_zh-CN.md) | 目录结构、技术栈说明 |
| [开发指南](docs/dev-guide_zh-CN.md) | 项目概述、核心模块、架构说明 |
| [API 参考](docs/api_zh-CN.md) | 全部接口、数据库设计 |
| [使用手册](docs/usage_zh-CN.md) | 界面说明、功能操作、快捷键 |
| [测试指南](docs/testing_zh-CN.md) | 后端/前端测试说明 |
| [部署指南](docs/deploy_zh-CN.md) | Docker 部署、环境变量 |
