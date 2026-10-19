use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use image::{ImageFormat, ImageReader};
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, OnceCell, Semaphore};

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::handlers::check::is_blocked_url;
use crate::result::ApiResult;
use crate::AppState;

const MAX_HTML_BYTES: usize = 512 * 1024;
const MAX_ICON_BYTES: usize = 64 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 3;
const MAX_PAGE_CANDIDATES: usize = 4;
const TOTAL_TIMEOUT_SECS: u64 = 8;
const CONCURRENCY: usize = 8;
const MAX_ICON_IDS: usize = 2000;
const MAX_SOURCE_ICON_DIM: u32 = 1024;
const TARGET_ICON_DIM: u32 = 64;
const LARGE_DATA_ICON_BYTES: usize = 8 * 1024;

type IconCell = Arc<OnceCell<Option<String>>>;
type IconCacheMap = Mutex<HashMap<String, IconCell>>;

/// Serves a locally stored icon as raw image bytes.
///
/// The URL includes a content hash (`/bookmark/icon/{id}/{hash}`), so the
/// response can be cached forever and a changed icon naturally gets a new URL.
/// The hash is also checked to avoid exposing icons by enumerating bookmark ids.
pub async fn get_icon(
    State(state): State<AppState>,
    Path((id, version)): Path<(i64, String)>,
) -> Result<Response, AppError> {
    let conn = state.db.get()?;
    let icon: Option<String> = conn
        .query_row("SELECT icon FROM bookmark WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .optional()?;

    let Some(icon) = icon.filter(|value| value.starts_with("data:")) else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    if icon_version(&icon) != version {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let Some((mime, bytes)) = decode_data_uri(&icon) else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // Stored SVGs may come from untrusted sites. The sandbox directive keeps
    // scripts from running if the icon URL is opened directly in a browser.
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; sandbox"),
    );
    Ok((headers, bytes).into_response())
}

/// Stable, cheap content hash used for icon URLs (FNV-1a 64-bit).
pub(crate) fn icon_version(icon: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in icon.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn decode_data_uri(icon: &str) -> Option<(&'static str, Vec<u8>)> {
    let rest = icon.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mut parts = meta.split(';');
    let mime = parts.next()?;
    if !parts.any(|part| part.eq_ignore_ascii_case("base64")) {
        return None;
    }
    let mime = match mime.to_ascii_lowercase().as_str() {
        "image/png" | "image/apng" => "image/png",
        "image/jpeg" | "image/jpg" => "image/jpeg",
        "image/gif" => "image/gif",
        "image/webp" => "image/webp",
        "image/bmp" => "image/bmp",
        "image/svg+xml" => "image/svg+xml",
        "image/x-icon" | "image/vnd.microsoft.icon" => "image/x-icon",
        _ => return None,
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    Some((mime, bytes))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchIconsRequest {
    pub ids: Option<Vec<i64>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchIconsResult {
    pub total: usize,
    pub updated: usize,
    pub failed: usize,
}

struct IconTarget {
    id: i64,
    url: String,
    icon: Option<String>,
}

struct Fetched {
    bytes: Vec<u8>,
    content_type: Option<String>,
    final_url: String,
}

/// Fetches icons for bookmarks whose icon is missing or still an external URL.
///
/// Icons are stored as `data:image/...;base64,...` in `bookmark.icon`, so the
/// browser never has to request an external favicon service again.
pub async fn fetch_icons(
    State(state): State<AppState>,
    _auth: AuthUser,
    payload: Option<Json<FetchIconsRequest>>,
) -> Result<ApiResult<FetchIconsResult>, AppError> {
    let ids = payload.and_then(|Json(req)| req.ids).map(|mut ids| {
        ids.truncate(MAX_ICON_IDS);
        ids
    });

    let targets = load_targets(&state, ids.as_deref())?;
    let total = targets.len();
    if targets.is_empty() {
        return Ok(ApiResult::success(FetchIconsResult {
            total: 0,
            updated: 0,
            failed: 0,
        }));
    }

    let client = match build_client() {
        Some(client) => client,
        None => return Ok(ApiResult::error("无法创建 HTTP 客户端")),
    };

    let allow_private = state.config.allow_private_urls;
    let semaphore = Arc::new(Semaphore::new(CONCURRENCY));
    let origin_cache: Arc<IconCacheMap> = Arc::new(Mutex::new(HashMap::new()));
    let db = state.db.clone();

    let mut handles = Vec::with_capacity(total);
    for target in targets {
        let client = client.clone();
        let semaphore = semaphore.clone();
        let cache = origin_cache.clone();
        let db = db.clone();

        handles.push(tokio::spawn(async move {
            let _permit = semaphore.acquire().await.ok()?;
            let icon = resolve_target_icon(&client, &target, allow_private, &cache).await?;

            // Persist each icon as soon as it is fetched. Spawned tasks keep
            // running even if the client disconnects, so partial progress is
            // never lost on a slow batch.
            let conn = db.get().ok()?;
            conn.execute(
                "UPDATE bookmark SET icon = ?1 WHERE id = ?2",
                rusqlite::params![icon, target.id],
            )
            .ok()?;
            Some(target.id)
        }));
    }

    let mut updated = 0;
    for handle in handles {
        match handle.await {
            Ok(Some(_)) => updated += 1,
            Ok(None) => {}
            Err(e) => tracing::warn!("icon fetch task failed: {e}"),
        }
    }

    tracing::info!("localized {updated}/{total} bookmark icons");

    Ok(ApiResult::success(FetchIconsResult {
        total,
        updated,
        failed: total - updated,
    }))
}

fn load_targets(state: &AppState, ids: Option<&[i64]>) -> Result<Vec<IconTarget>, AppError> {
    let conn = state.db.get()?;
    let mut targets = Vec::new();

    if let Some(ids) = ids {
        if ids.is_empty() {
            return Ok(targets);
        }
        let placeholders = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("SELECT id, url, icon FROM bookmark WHERE id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |row| {
            Ok(IconTarget {
                id: row.get(0)?,
                url: row.get(1)?,
                icon: row.get(2)?,
            })
        })?;
        for row in rows {
            let target = row?;
            if needs_icon(target.icon.as_deref()) || is_large_data_icon(target.icon.as_deref()) {
                targets.push(target);
            }
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, url, icon FROM bookmark
             WHERE icon IS NULL
                OR TRIM(icon) = ''
                OR icon LIKE 'http://%'
                OR icon LIKE 'https://%'
                OR (icon LIKE 'data:%' AND LENGTH(icon) > ?1
                    AND icon NOT LIKE 'data:image/svg+xml%')",
        )?;
        let rows = stmt.query_map([LARGE_DATA_ICON_BYTES], |row| {
            Ok(IconTarget {
                id: row.get(0)?,
                url: row.get(1)?,
                icon: row.get(2)?,
            })
        })?;
        for row in rows {
            targets.push(row?);
        }
    }

    Ok(targets)
}

async fn resolve_target_icon(
    client: &Client,
    target: &IconTarget,
    allow_private: bool,
    cache: &IconCacheMap,
) -> Option<String> {
    // A manually entered external icon URL: download that exact image first.
    if let Some(icon) = target.icon.as_deref() {
        let trimmed = icon.trim();
        if trimmed.starts_with("data:") {
            // Large legacy base64 icons are re-encoded to a small PNG.
            return normalize_data_icon(trimmed);
        }
        if is_http_url(trimmed) {
            if let Some(data) = fetch_image_as_data_uri(client, trimmed, allow_private).await {
                return Some(data);
            }
        }
    }

    let page = normalize_url(&target.url)?;
    let origin = page.origin().ascii_serialization();

    // Deduplicate by origin: many bookmarks usually share the same domain.
    let cell = {
        let mut cache = cache.lock().await;
        cache
            .entry(origin)
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone()
    };

    cell.get_or_init(|| async { fetch_icon_data_uri(client, page.as_str(), allow_private).await })
        .await
        .clone()
}

pub(crate) fn needs_icon(icon: Option<&str>) -> bool {
    match icon {
        None => true,
        Some(value) => {
            let value = value.trim();
            value.is_empty() || is_http_url(value)
        }
    }
}

fn is_large_data_icon(icon: Option<&str>) -> bool {
    icon.is_some_and(|value| {
        value.starts_with("data:")
            && value.len() > LARGE_DATA_ICON_BYTES
            && !value.starts_with("data:image/svg+xml")
    })
}

fn is_http_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn build_client() -> Option<Client> {
    Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .user_agent(format!("subnav/{}", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .ok()
}

/// Downloads the page, discovers icon candidates, and returns the first usable
/// image as a data URI. External favicon services are only contacted by the
/// server as a last resort; their bytes are stored locally.
async fn fetch_icon_data_uri(
    client: &Client,
    page_url: &str,
    allow_private: bool,
) -> Option<String> {
    tokio::time::timeout(
        Duration::from_secs(TOTAL_TIMEOUT_SECS),
        fetch_icon_data_uri_inner(client, page_url, allow_private),
    )
    .await
    .ok()
    .flatten()
}

async fn fetch_icon_data_uri_inner(
    client: &Client,
    page_url: &str,
    allow_private: bool,
) -> Option<String> {
    let page = normalize_url(page_url)?;
    let mut base = page.clone();
    let mut candidates = Vec::new();

    if let Ok(fetched) = get_capped(client, page.as_str(), MAX_HTML_BYTES, allow_private).await {
        if let Some(mime) = sniff_image_mime(&fetched.bytes, None) {
            // The bookmark URL itself is an image.
            return Some(to_data_uri(mime, &fetched.bytes));
        }
        if let Ok(final_url) = url::Url::parse(&fetched.final_url) {
            base = final_url;
        }
        let html = String::from_utf8_lossy(&fetched.bytes);
        candidates = extract_icon_candidates(&html, &fetched.final_url);
    }

    let favicon = format!("{}/favicon.ico", base.origin().ascii_serialization());
    if !candidates.contains(&favicon) {
        candidates.push(favicon);
    }

    for candidate in candidates.iter().take(MAX_PAGE_CANDIDATES + 1) {
        if let Some(data) = fetch_image_as_data_uri(client, candidate, allow_private).await {
            return Some(data);
        }
    }

    for candidate in external_icon_candidates(&base) {
        if let Some(data) = fetch_image_as_data_uri(client, &candidate, allow_private).await {
            return Some(data);
        }
    }

    None
}

async fn fetch_image_as_data_uri(
    client: &Client,
    url: &str,
    allow_private: bool,
) -> Option<String> {
    let fetched = get_capped(client, url, MAX_ICON_BYTES, allow_private)
        .await
        .ok()?;
    let mime = sniff_image_mime(&fetched.bytes, fetched.content_type.as_deref())?;
    if let Some(normalized) = normalize_icon(&fetched.bytes, mime) {
        return Some(to_data_uri("image/png", &normalized));
    }
    Some(to_data_uri(mime, &fetched.bytes))
}

/// Re-encodes raster icons to a small PNG. Large favicons (512×512) are very
/// expensive to decode/paint at an 18px display size and cause scroll jank.
fn normalize_icon(bytes: &[u8], mime: &str) -> Option<Vec<u8>> {
    if mime == "image/svg+xml" {
        return None;
    }

    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0 || height == 0 || width > MAX_SOURCE_ICON_DIM || height > MAX_SOURCE_ICON_DIM {
        return None;
    }
    // Already a display-sized PNG: re-encoding would only churn its hash.
    if mime == "image/png" && width <= TARGET_ICON_DIM && height <= TARGET_ICON_DIM {
        return None;
    }

    let image = image::load_from_memory(bytes).ok()?;
    let thumbnail = image.thumbnail(TARGET_ICON_DIM, TARGET_ICON_DIM);
    let mut output = Cursor::new(Vec::new());
    thumbnail.write_to(&mut output, ImageFormat::Png).ok()?;
    let output = output.into_inner();
    (output.len() < bytes.len()).then_some(output)
}

fn normalize_data_icon(icon: &str) -> Option<String> {
    let (mime, bytes) = decode_data_uri(icon)?;
    let normalized = normalize_icon(&bytes, mime)?;
    let data = to_data_uri("image/png", &normalized);
    (data.len() < icon.len()).then_some(data)
}

/// Shrinks oversized base64 icons already stored in the database.
pub fn normalize_existing_icons(pool: &crate::db::DbPool) {
    let conn = match pool.get() {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!("failed to normalize stored icons: {error}");
            return;
        }
    };

    let mut statement = match conn.prepare(
        "SELECT id, icon FROM bookmark
         WHERE icon LIKE 'data:%' AND LENGTH(icon) > ?1
           AND icon NOT LIKE 'data:image/svg+xml%'",
    ) {
        Ok(statement) => statement,
        Err(error) => {
            tracing::warn!("failed to query stored icons: {error}");
            return;
        }
    };
    let mapped = match statement.query_map([LARGE_DATA_ICON_BYTES], |row| {
        Ok((row.get(0)?, row.get(1)?))
    }) {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!("failed to scan stored icons: {error}");
            return;
        }
    };
    let rows: Vec<(i64, String)> = match mapped.collect::<Result<Vec<_>, _>>() {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!("failed to read stored icons: {error}");
            return;
        }
    };
    drop(statement);

    let updates: Vec<(i64, String)> = rows
        .iter()
        .filter_map(|(id, icon)| normalize_data_icon(icon).map(|icon| (*id, icon)))
        .collect();
    if updates.is_empty() {
        return;
    }

    let transaction = match conn.unchecked_transaction() {
        Ok(transaction) => transaction,
        Err(error) => {
            tracing::warn!("failed to start icon normalization transaction: {error}");
            return;
        }
    };

    let mut updated = 0;
    for (id, icon) in &updates {
        if transaction
            .execute(
                "UPDATE bookmark SET icon = ?1 WHERE id = ?2",
                rusqlite::params![icon, id],
            )
            .is_ok()
        {
            updated += 1;
        }
    }

    match transaction.commit() {
        Ok(()) => tracing::info!("normalized {updated} stored icons"),
        Err(error) => tracing::warn!("failed to commit icon normalization: {error}"),
    }
}

async fn get_capped(
    client: &Client,
    url: &str,
    max_bytes: usize,
    allow_private: bool,
) -> Result<Fetched, String> {
    if is_blocked_url(url, allow_private).await {
        return Err("blocked: private or local address".to_string());
    }

    let mut response = client.get(url).send().await.map_err(|e| e.to_string())?;
    let final_url = response.url().to_string();
    if is_blocked_url(&final_url, allow_private).await {
        return Err("blocked: redirect to private address".to_string());
    }
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    if let Some(length) = response.content_length() {
        if length > max_bytes as u64 {
            return Err("response too large".to_string());
        }
    }

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > max_bytes {
            return Err("response too large".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }

    Ok(Fetched {
        bytes,
        content_type,
        final_url,
    })
}

fn normalize_url(url: &str) -> Option<url::Url> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let parsed = url::Url::parse(&with_scheme).ok()?;
    matches!(parsed.scheme(), "http" | "https").then_some(parsed)
}

fn external_icon_candidates(page: &url::Url) -> Vec<String> {
    let Some(host) = page.host_str() else {
        return vec![];
    };
    let scheme = page.scheme();
    vec![
        format!("https://www.google.com/s2/favicons?sz=64&domain_url={scheme}://{host}"),
        format!("https://icons.duckduckgo.com/ip3/{host}.ico"),
    ]
}

fn to_data_uri(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn sniff_image_mime(bytes: &[u8], declared: Option<&str>) -> Option<&'static str> {
    if bytes.starts_with(b"\x1f\x8b") {
        // Compressed body: we never request gzip, so treat it as unusable.
        return None;
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.starts_with(&[0, 0, 1, 0]) {
        return Some("image/x-icon");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.len() >= 14 && bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }

    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
    let head = head.trim_start_matches('\u{feff}').trim_start();
    let lower_head = head.to_ascii_lowercase();
    if lower_head.starts_with("<!doctype html") || lower_head.starts_with("<html") {
        // A mislabeled HTML error page, not an image.
        return None;
    }
    if head.starts_with("<svg") || (head.starts_with("<?xml") && head.contains("<svg")) {
        return Some("image/svg+xml");
    }

    match declared
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        })
        .as_deref()
    {
        Some("image/png") | Some("image/apng") => Some("image/png"),
        Some("image/jpeg") | Some("image/jpg") => Some("image/jpeg"),
        Some("image/gif") => Some("image/gif"),
        Some("image/webp") => Some("image/webp"),
        Some("image/bmp") => Some("image/bmp"),
        Some("image/svg+xml") => Some("image/svg+xml"),
        Some("image/x-icon") | Some("image/vnd.microsoft.icon") => Some("image/x-icon"),
        _ => None,
    }
}

fn extract_icon_candidates(html: &str, base_url: &str) -> Vec<String> {
    let Ok(base) = url::Url::parse(base_url) else {
        return vec![];
    };

    // (size, is_regular_icon, document order, url)
    let mut candidates: Vec<(u32, u8, usize, String)> = Vec::new();

    for (order, tag) in extract_link_tags(html).into_iter().enumerate() {
        let rel = extract_attr(tag, "rel")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let tokens: Vec<&str> = rel.split_whitespace().collect();
        let is_regular = tokens.contains(&"icon");
        let is_apple = tokens
            .iter()
            .any(|token| token.starts_with("apple-touch-icon"));
        if !is_regular && !is_apple {
            continue;
        }

        let Some(href) = extract_attr(tag, "href") else {
            continue;
        };
        let href = href.trim();
        if href.is_empty() {
            continue;
        }
        let Ok(resolved) = base.join(href) else {
            continue;
        };
        if !matches!(resolved.scheme(), "http" | "https") {
            continue;
        }

        let size = extract_attr(tag, "sizes")
            .map(|value| parse_max_size(&value))
            .unwrap_or(0);
        let priority = u8::from(is_regular);
        candidates.push((size, priority, order, resolved.to_string()));
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));

    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter_map(|(_, _, _, url)| seen.insert(url.clone()).then_some(url))
        .collect()
}

fn parse_max_size(sizes: &str) -> u32 {
    sizes
        .split_whitespace()
        .filter_map(|size| {
            if size.eq_ignore_ascii_case("any") {
                return Some(1024);
            }
            size.split('x').next()?.parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0)
}

fn extract_link_tags(html: &str) -> Vec<&str> {
    let lower = html.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut tags = Vec::new();
    let mut from = 0;

    while let Some(relative) = lower[from..].find("<link") {
        let start = from + relative;
        let after = start + "<link".len();
        let boundary = bytes
            .get(after)
            .map(|byte| byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/')
            .unwrap_or(true);
        if !boundary {
            from = after;
            continue;
        }

        match find_tag_end(&html[start..]) {
            Some(end) => {
                tags.push(&html[start..start + end + 1]);
                from = start + end + 1;
            }
            None => break,
        }
    }

    tags
}

fn find_tag_end(tag: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (index, byte) in tag.bytes().enumerate() {
        match quote {
            Some(current) => {
                if byte == current {
                    quote = None;
                }
            }
            None => match byte {
                b'"' | b'\'' => quote = Some(byte),
                b'>' => return Some(index),
                _ => {}
            },
        }
    }
    None
}

/// Extracts an attribute value, ignoring case, attribute order, and optional
/// whitespace around `=`. Supports double, single, and unquoted values.
fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut from = 0;

    while let Some(relative) = lower[from..].find(name) {
        let start = from + relative;
        let before_ok =
            start == 0 || bytes[start - 1].is_ascii_whitespace() || bytes[start - 1] == b'<';
        let after_name = start + name.len();
        if !before_ok {
            from = after_name;
            continue;
        }

        let mut index = after_name;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if bytes.get(index) != Some(&b'=') {
            from = after_name;
            continue;
        }

        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            return None;
        }

        let quote = bytes[index];
        if quote == b'"' || quote == b'\'' {
            let rest = &tag[index + 1..];
            return rest.find(quote as char).map(|end| rest[..end].to_string());
        }

        let rest = &tag[index..];
        let end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '>')
            .unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        return Some(rest[..end].to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_icon_detects_missing_and_external_values() {
        assert!(needs_icon(None));
        assert!(needs_icon(Some("")));
        assert!(needs_icon(Some("  ")));
        assert!(needs_icon(Some("https://www.google.com/s2/favicons?x=1")));
        assert!(needs_icon(Some("HTTP://example.com/icon.png")));
        assert!(!needs_icon(Some("data:image/png;base64,AAAA")));
    }

    #[test]
    fn extracts_and_sorts_icon_candidates() {
        let html = r#"
            <html><head>
              <link rel="icon" sizes="16x16" href="/small.png">
              <link rel="apple-touch-icon" sizes="180x180" href="https://cdn.example.com/apple.png">
              <link rel="icon" sizes="32x32" type="image/png" href="/big.png">
              <link rel="mask-icon" href="/mask.svg">
              <link rel="stylesheet" href="/style.css">
            </head></html>
        "#;
        let candidates = extract_icon_candidates(html, "https://example.com/page");
        assert_eq!(
            candidates,
            vec![
                "https://cdn.example.com/apple.png",
                "https://example.com/big.png",
                "https://example.com/small.png",
            ]
        );
    }

    #[test]
    fn resolves_relative_and_protocol_relative_urls() {
        let html = r#"<link rel="shortcut icon" href="//cdn.example.com/fav.ico">"#;
        let candidates = extract_icon_candidates(html, "https://example.com/a/b");
        assert_eq!(candidates, vec!["https://cdn.example.com/fav.ico"]);
    }

    #[test]
    fn ignores_invalid_and_non_http_candidates() {
        let html = r#"
            <link rel="icon" href="">
            <link rel="icon" href="data:image/png;base64,AAAA">
            <link rel="icon" href="javascript:alert(1)">
        "#;
        assert!(extract_icon_candidates(html, "https://example.com").is_empty());
    }

    #[test]
    fn attribute_parsing_handles_case_spacing_and_quotes() {
        let tag = r#"<LINK REL = 'icon' HREF = "https://x.com/fav.ico" sizes=32x32>"#;
        assert_eq!(extract_attr(tag, "rel").as_deref(), Some("icon"));
        assert_eq!(
            extract_attr(tag, "href").as_deref(),
            Some("https://x.com/fav.ico")
        );
        assert_eq!(extract_attr(tag, "sizes").as_deref(), Some("32x32"));
        assert_eq!(extract_attr(tag, "missing"), None);
    }

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_max_size("16x16"), 16);
        assert_eq!(parse_max_size("16x16 32x32"), 32);
        assert_eq!(parse_max_size("any"), 1024);
        assert_eq!(parse_max_size("garbage"), 0);
    }

    #[test]
    fn sniffs_common_image_formats() {
        assert_eq!(
            sniff_image_mime(b"\x89PNG\r\n\x1a\nrest", None),
            Some("image/png")
        );
        assert_eq!(
            sniff_image_mime(b"\xff\xd8\xffrest", None),
            Some("image/jpeg")
        );
        assert_eq!(sniff_image_mime(b"GIF89a....", None), Some("image/gif"));
        assert_eq!(
            sniff_image_mime(&[0, 0, 1, 0, 9], None),
            Some("image/x-icon")
        );
        assert_eq!(sniff_image_mime(b"RIFF0000WEBP", None), Some("image/webp"));
        assert_eq!(
            sniff_image_mime(b"<?xml version=\"1.0\"?><svg></svg>", None),
            Some("image/svg+xml")
        );
        assert_eq!(sniff_image_mime(b"not an image", None), None);
        assert_eq!(
            sniff_image_mime(b"not an image", Some("image/png")),
            Some("image/png")
        );
        assert_eq!(
            sniff_image_mime(b"\x1f\x8bcompressed", Some("image/png")),
            None
        );
        assert_eq!(
            sniff_image_mime(b"<!DOCTYPE html><html></html>", Some("image/png")),
            None
        );
    }

    #[test]
    fn builds_base64_data_uri() {
        assert_eq!(
            to_data_uri("image/png", b"abc"),
            "data:image/png;base64,YWJj"
        );
    }

    #[test]
    fn decodes_data_uri_roundtrip() {
        let icon = to_data_uri("image/png", b"abc");
        let (mime, bytes) = decode_data_uri(&icon).unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(bytes, b"abc");

        assert!(decode_data_uri("https://example.com/fav.ico").is_none());
        assert!(decode_data_uri("data:text/plain;base64,YWJj").is_none());
        assert!(decode_data_uri("data:image/png,YWJj").is_none());
        assert!(decode_data_uri("data:image/png;base64,!!!").is_none());
    }

    #[test]
    fn icon_version_is_stable_and_content_sensitive() {
        assert_eq!(icon_version("abc"), icon_version("abc"));
        assert_ne!(icon_version("abc"), icon_version("abd"));
        assert_eq!(icon_version("abc").len(), 16);
    }

    #[test]
    fn normalizes_large_raster_icons() {
        use std::io::Cursor;

        let mut image = image::RgbaImage::new(512, 512);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgba([
                (x % 256) as u8,
                (y % 256) as u8,
                ((x * 7 + y * 13) % 256) as u8,
                255,
            ]);
        }
        let mut source = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut source, ImageFormat::Png)
            .unwrap();
        let bytes = source.into_inner();

        let normalized = normalize_icon(&bytes, "image/png").unwrap();
        assert!(normalized.len() < bytes.len());
        let decoded = image::load_from_memory(&normalized).unwrap();
        assert!(decoded.width() <= TARGET_ICON_DIM);
        assert!(decoded.height() <= TARGET_ICON_DIM);

        let data = to_data_uri("image/png", &bytes);
        let normalized_data = normalize_data_icon(&data).unwrap();
        assert!(normalized_data.starts_with("data:image/png;base64,"));
        assert!(normalized_data.len() < data.len());

        assert!(normalize_icon(b"<svg></svg>", "image/svg+xml").is_none());
    }

    #[test]
    fn normalizes_urls() {
        assert_eq!(
            normalize_url("example.com").map(|u| u.to_string()),
            Some("https://example.com/".to_string())
        );
        assert!(normalize_url("javascript:alert(1)").is_none());
        assert!(normalize_url("   ").is_none());
    }

    #[tokio::test]
    async fn fetches_icon_from_local_page() {
        use axum::routing::get;
        use axum::Router;

        let png: Vec<u8> = b"\x89PNG\r\n\x1a\n\0\0\0\0".to_vec();
        let page = r#"<html><head><link rel="icon" href="/icon.png"></head></html>"#;

        let app = Router::new()
            .route("/page", get(move || async move { page }))
            .route(
                "/icon.png",
                get(move || {
                    let png = png.clone();
                    async move { ([(CONTENT_TYPE, "image/png")], png) }
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = build_client().unwrap();
        let icon = fetch_icon_data_uri(&client, &format!("http://{addr}/page"), true).await;
        assert_eq!(
            icon,
            Some(to_data_uri("image/png", b"\x89PNG\r\n\x1a\n\0\0\0\0"))
        );
    }
}
