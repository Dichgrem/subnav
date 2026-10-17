# 测试指南

## 运行测试

```bash
just test            # 运行全部测试（后端 + 前端）
just test-backend    # 仅后端：cargo test
just test-frontend   # 仅前端：pnpm test
```

## 测试框架

| 端 | 框架 | 说明 |
|----|------|------|
| 后端 | Rust 内置 `#[test]` | 29 个单元测试，零额外依赖 |
| 前端 | Vitest 4 + @vue/test-utils + happy-dom | 14 个单元测试，浏览器环境模拟 |

## 后端测试（29 个）

位于 `backend/src/` 各模块内，以 `#[cfg(test)] mod tests` 内联。

### auth.rs（3 个）

| 测试 | 说明 |
|------|------|
| `sign_and_verify_roundtrip` | 签发 JWT → 验证 → 断言 user_id / role 一致 |
| `verify_wrong_secret` | 用错误密钥验证 → 应失败 |
| `verify_tampered_token` | 篡改 token → 验证应失败 |

### handlers/category.rs（2 个）

| 测试 | 说明 |
|------|------|
| `collect_single_category` | 单层分类，仅收集自身 id |
| `collect_nested_categories` | 嵌套分类树（3 层），收集自身 + 所有子孙 id |

### handlers/bookmark.rs（5 个）

HTML 导入导出逻辑虽已拆至 `handlers/import.rs` / `handlers/export.rs`，但相关测试仍集中在 bookmark.rs 的测试模块中。

| 测试 | 说明 |
|------|------|
| `import_nested_html` | 多层 `<DL><DT><H3>` 嵌套解析 + 无分类书签 |
| `import_flat_html_no_folders` | 扁平 `<DT><A>` 列表解析，不建分类 |
| `import_duplicate_url_skipped` | 已存在 URL 跳过（不重复导入） |
| `list_bookmarks_ordered_by_sort` | 多书签查询不报错（排序由 handler SQL 负责） |
| `export_as_html_includes_all` | 导出 HTML 包含分类与书签 |

### handlers/icon.rs（10 个）

| 测试 | 说明 |
|------|------|
| `extracts_and_sorts_icon_candidates` | 解析 `<link rel=icon>`，按尺寸/优先级排序并去重 |
| `resolves_relative_and_protocol_relative_urls` | 相对路径与协议相对 URL 正确解析 |
| `ignores_invalid_and_non_http_candidates` | 空值、`data:`、`javascript:` 被忽略 |
| `attribute_parsing_handles_case_spacing_and_quotes` | 属性大小写/空格/引号解析 |
| `parses_sizes` | `sizes` 属性解析（16x16 / any / 非法值） |
| `sniffs_common_image_formats` | PNG/JPEG/GIF/ICO/WEBP/SVG 魔数识别 |
| `builds_base64_data_uri` | base64 data URI 编码 |
| `normalizes_urls` | 无协议 URL 补全、非法协议拒绝 |
| `needs_icon_detects_missing_and_external_values` | 空图标/外链图标判定 |
| `fetches_icon_from_local_page` | 本地 axum 服务端到端抓取页面图标 |

## 前端测试（14 个）

位于 `frontend/src/__tests__/`。

### router.test.js（3 个）

| 测试 | 说明 |
|------|------|
| 未登录访问首页 | 重定向到 `/login` |
| 已登录访问首页 | 正常渲染，不重定向 |
| 已登录访问登录页 | 重定向到 `/` |

### tree.test.js（5 个）

| 测试 | 说明 |
|------|------|
| 空数组输入 | 返回 `[]` |
| 扁平列表（无嵌套） | 正确按 `parentId` 分组 |
| 多级嵌套 | 正确构建嵌套树结构 |
| sortOrder 排序 | 子节点按 `sortOrder` 升序排列 |
| 保留额外字段 | 原始对象其他属性不丢失 |

## 代码质量

提交前运行检查和自动修复：

```bash
just lint    # 只读检查：cargo clippy + prettier --check
just fmt     # 自动修复：cargo fmt + clippy --fix + prettier --write + alejandra
```
