# Changelog

All notable changes to subnav.

## [0.4.0] - 2026-10-17

### Added
- Icons are now downloaded server-side and stored in the database as `data:image/...;base64,...`; the browser no longer requests Google/DuckDuckGo favicons on every visit
- `POST /bookmark/fetchIcons` accepts optional `ids` and returns `{total, updated, failed}`
- One-time background migration rewrites legacy external icon URLs to local data URIs
- Letter placeholder for bookmarks whose icon cannot be fetched

### Changed
- Saving a bookmark no longer blocks on icon fetching; localization runs in the background after save
- Removed `extract_favicon_url()` and the browser-side DuckDuckGo fallback

### Security
- Icon fetching reuses the link checker's SSRF protection; HTML is capped at 512KB and icons at 64KB, with magic-byte image validation

## [0.3.1] - 2026-09-04

### Changed
- Migrate to single-user mode: remove user_id/role columns, AdminUser, multi-user APIs and frontend user management UI (keep login/JWT/change password/env-based bootstrap)
- HTML import parsing is now case-insensitive, decodes entities, and prefers ICON_URI over bulky data-URI icons

### Fixed
- Export order now follows sort_order (was insertion order)
- Router pre-checks JWT expiry to avoid wasted requests to protected pages
- Migrate remaining hardcoded Chinese UI text to i18n

### BREAKING
- Database schema changed; delete existing database files to recreate

## [0.3.0] - 2026-09-03

### Security
- SSRF protection: reject private/loopback/link-local addresses in link checker
- Login rate limiting (5 failures → 15 min lockout)
- Replace public register with env-based admin bootstrap (`ADMIN_USERNAME`/`ADMIN_PASSWORD`)

### Fixed
- Infinite polling when checking links with no bookmarks
- Link checker concurrency (unified tokio mutex, task error handling, ordered results)
- 401 handling: clear session and redirect to login
- Favicon fallback to DuckDuckGo when Google is unavailable
- Error handling for save operations and user parsing
- Export via axios to go through interceptors

### Changed
- JWT default expiry extended to 30 days
- GitHub Actions upgraded to Node 24 versions

## [0.2.0] - 2026-07-21

### Added
- Fuzzy search with pinyin support (Fuse.js + pinyin-pro)
- Backend and frontend test suites (11 Rust tests, 7 Vitest tests)
- `r2d2` connection pool with transactions and DB indexes
- Parallel bookmark/category loading on startup
- Server timeout (Tower middleware)

### Changed
- Element Plus switched to on-demand imports (unplugin-vue-components)
- `pinyin-pro` lazy-loaded for smaller initial bundle
- Main bundle code-split into separate chunks
- Drag delay reduced by 50ms
- HTML import rewritten with line-by-line parser

### Fixed
- `list_categories` and `list_bookmarks_raw` unwrap → `?` error propagation
- Dead `ALTER TABLE` hacks and stale CSS/docs removed
- Docker DB path mismatch resolved, `rusqlite::Transaction` API used
- Vue Router `next()` replaced with `return` pattern

### Internal
- `useBookmarks` and `useDragDrop` composables extracted from HomeView
- Justfile: `fmt/check/fix` merged into `lint` and `fmt`
- Docs restructured into 6 bilingual Chinese files

## [0.1.0] - 2026-06-05

### Added
- User system: registration, login, JWT auth, admin role (first registrant)
- Category management: CRUD, tree view with collapse/expand, drag-to-sort
- Bookmark management: CRUD, drag-to-sort with undo, category assignment
- JSON import/export
- HTML bookmarks import/export (Netscape format, Chrome/Firefox/Edge compatible)
- Favicon auto-fetch and favicon column in bookmark list
- Full-text search across title and URL
- i18n support (zh-CN / English)
- Dark/light theme toggle
- Password change dialog for all users
- Admin panel: add/delete users
- Keyboard shortcuts: `Ctrl+K` focus search, `Escape` close dialogs
- Nix flake development environment with `just` task runner
- Docker multi-stage build (musl + scratch, ~15MB final image)
- Docker Compose for one-command deployment

### Changed
- Backend: Java/Spring/MariaDB → Rust/Axum/SQLite rewrite
- Frontend: Vue 2 → Vue 3 Composition API + Element Plus
- JWT secret auto-generated on first startup (no config required)
- Passwords hashed with bcrypt
- Input validation on all endpoints
