use crate::auth::AuthUser;
use crate::error::AppError;
use crate::handlers::export::export_as_html;
use crate::handlers::icon::icon_version;
use crate::handlers::import::import_from_html;
use crate::models::{Bookmark, Category};
use crate::result::ApiResult;
use crate::AppState;
use axum::{
    extract::{Multipart, State},
    http::header,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct DeleteRequest {
    pub id: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSortItem {
    pub id: i64,
    pub sort_order: i32,
}

pub async fn list(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<ApiResult<Vec<Bookmark>>, AppError> {
    let conn = state.db.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, title, url, icon, category_id, sort_order
         FROM bookmark ORDER BY sort_order ASC",
    )?;
    let mut bookmarks: Vec<Bookmark> = stmt
        .query_map([], |row| {
            Ok(Bookmark {
                id: Some(row.get(0)?),
                title: row.get(1)?,
                url: row.get(2)?,
                icon: row.get(3)?,
                category_id: row.get(4)?,
                sort_order: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // Keep the list payload small: embedded base64 icons are served by the
    // dedicated, long-cacheable /bookmark/icon/{id}/{hash} endpoint.
    for bookmark in &mut bookmarks {
        let endpoint = match (bookmark.id, bookmark.icon.as_deref()) {
            (Some(id), Some(icon)) if icon.starts_with("data:") => {
                Some(format!("/bookmark/icon/{id}/{}", icon_version(icon)))
            }
            _ => None,
        };
        if let Some(url) = endpoint {
            bookmark.icon = Some(url);
        }
    }

    Ok(ApiResult::success(bookmarks))
}

pub async fn add(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(mut bookmark): Json<Bookmark>,
) -> Result<ApiResult<Bookmark>, AppError> {
    bookmark.title = bookmark.title.trim().to_string();
    bookmark.url = bookmark.url.trim().to_string();
    if bookmark.title.is_empty() || bookmark.url.is_empty() {
        return Ok(ApiResult::error("标题和网址不能为空"));
    }
    if bookmark.title.len() > 500 || bookmark.url.len() > 2000 {
        return Ok(ApiResult::error("内容过长"));
    }
    bookmark.icon = sanitize_icon(bookmark.icon);
    let conn = state.db.get()?;

    if let Some(id) = bookmark.id {
        // icon = NULL keeps the existing icon (the list only exposes a short
        // /bookmark/icon/... URL), icon = "" clears it, any other value sets it.
        conn.execute(
            "UPDATE bookmark
             SET title=?1, url=?2,
                 icon=CASE WHEN ?3 IS NULL THEN icon ELSE ?3 END,
                 category_id=?4, sort_order=?5
             WHERE id=?6",
            rusqlite::params![
                bookmark.title,
                bookmark.url,
                bookmark.icon,
                bookmark.category_id,
                bookmark.sort_order,
                id,
            ],
        )?;
        let updated = load_bookmark(&conn, id)?;
        Ok(ApiResult::success(updated))
    } else {
        conn.execute(
            "INSERT INTO bookmark (title, url, icon, category_id, sort_order) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                bookmark.title,
                bookmark.url,
                bookmark.icon,
                bookmark.category_id,
                bookmark.sort_order,
            ],
        )?;
        let id = conn.last_insert_rowid();
        if bookmark.sort_order == 0 {
            conn.execute(
                "UPDATE bookmark SET sort_order = id WHERE id = ?1",
                rusqlite::params![id],
            )?;
            bookmark.sort_order = id as i32;
        }
        Ok(ApiResult::success(Bookmark {
            id: Some(id),
            title: bookmark.title,
            url: bookmark.url,
            icon: bookmark.icon,
            category_id: bookmark.category_id,
            sort_order: bookmark.sort_order,
        }))
    }
}

fn load_bookmark(conn: &rusqlite::Connection, id: i64) -> Result<Bookmark, rusqlite::Error> {
    conn.query_row(
        "SELECT id, title, url, icon, category_id, sort_order FROM bookmark WHERE id = ?1",
        [id],
        |row| {
            Ok(Bookmark {
                id: Some(row.get(0)?),
                title: row.get(1)?,
                url: row.get(2)?,
                icon: row.get(3)?,
                category_id: row.get(4)?,
                sort_order: row.get(5)?,
            })
        },
    )
}

fn sanitize_icon(icon: Option<String>) -> Option<String> {
    match icon {
        Some(value) if value.trim().starts_with("/bookmark/icon/") => None,
        Some(value) => Some(value.trim().to_string()),
        None => None,
    }
}

pub async fn batch_update_sort(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(items): Json<Vec<BatchSortItem>>,
) -> Result<ApiResult<()>, AppError> {
    if items.len() > 1000 {
        return Ok(ApiResult::error("单次更新数量不能超过 1000"));
    }
    if items.is_empty() {
        return Ok(ApiResult::success_empty());
    }
    let mut conn = state.db.get()?;
    let tx = conn.transaction()?;

    for item in &items {
        tx.execute(
            "UPDATE bookmark SET sort_order = ?1 WHERE id = ?2",
            rusqlite::params![item.sort_order, item.id],
        )?;
    }
    tx.commit()?;
    Ok(ApiResult::success_empty())
}

pub async fn delete(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(req): Json<DeleteRequest>,
) -> Result<ApiResult<()>, AppError> {
    let conn = state.db.get()?;
    conn.execute(
        "DELETE FROM bookmark WHERE id = ?1",
        rusqlite::params![req.id],
    )?;
    Ok(ApiResult::success_empty())
}

pub async fn export_bookmarks(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let conn = state.db.get()?;
    let categories = list_categories(&conn)?;
    let bookmarks = list_bookmarks_raw(&conn)?;
    let html = export_as_html(&categories, &bookmarks);
    let bytes = html.into_bytes();
    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=UTF-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"bookmarks.html\"",
            ),
        ],
        bytes,
    ))
}

pub async fn import_bookmarks(
    State(state): State<AppState>,
    _auth: AuthUser,
    mut multipart: Multipart,
) -> Result<ApiResult<i64>, AppError> {
    let mut file_content = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            file_content = field
                .bytes()
                .await
                .ok()
                .and_then(|b| String::from_utf8(b.to_vec()).ok());
        }
    }

    let content = match file_content {
        Some(c) => c,
        None => return Ok(ApiResult::error("未上传文件")),
    };

    let conn = state.db.get()?;
    match import_from_html(&conn, &content) {
        Ok(count) => Ok(ApiResult::success(count)),
        Err(e) => Ok(ApiResult::error(format!("导入失败: {}", e))),
    }
}

fn list_categories(conn: &rusqlite::Connection) -> Result<Vec<Category>, rusqlite::Error> {
    let mut stmt = conn
        .prepare("SELECT id, name, parent_id, sort_order FROM category ORDER BY sort_order ASC")?;
    let items: Vec<Category> = stmt
        .query_map([], |row| {
            Ok(Category {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                parent_id: row.get(2)?,
                sort_order: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(items)
}

fn list_bookmarks_raw(conn: &rusqlite::Connection) -> Result<Vec<Bookmark>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, title, url, icon, category_id, sort_order FROM bookmark ORDER BY sort_order ASC",
    )?;
    let items: Vec<Bookmark> = stmt
        .query_map([], |row| {
            Ok(Bookmark {
                id: Some(row.get(0)?),
                title: row.get(1)?,
                url: row.get(2)?,
                icon: row.get(3)?,
                category_id: row.get(4)?,
                sort_order: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_icon_ignores_local_endpoint_urls() {
        assert_eq!(sanitize_icon(Some("/bookmark/icon/1/abc".into())), None);
        assert_eq!(
            sanitize_icon(Some(" https://example.com/fav.ico ".into())),
            Some("https://example.com/fav.ico".into())
        );
        assert_eq!(sanitize_icon(Some(String::new())), Some(String::new()));
        assert_eq!(sanitize_icon(None), None);
    }

    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE category (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                parent_id INTEGER,
                sort_order INTEGER DEFAULT 0
            );
            CREATE TABLE bookmark (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                url TEXT NOT NULL,
                icon TEXT,
                category_id INTEGER,
                sort_order INTEGER DEFAULT 0
            );
            ",
        )
        .unwrap();
        conn
    }

    #[test]
    fn import_nested_html() {
        let conn = setup_db();
        let html = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<TITLE>Bookmarks</TITLE>
<H1>书签菜单</H1>
<DL><p>
    <DT><H3>Dev</H3>
    <DL><p>
        <DT><A HREF="https://github.com">GitHub</A>
    </DL><p>
    <DT><A HREF="https://example.com">Root</A>
</DL><p>
"#;
        let count = import_from_html(&conn, html).unwrap();
        assert_eq!(count, 2);

        let categories = list_categories(&conn).unwrap();
        assert_eq!(categories.len(), 1);
        assert_eq!(categories[0].name, "Dev");

        let bookmarks = list_bookmarks_raw(&conn).unwrap();
        assert_eq!(bookmarks.len(), 2);
    }

    #[test]
    fn import_flat_html_no_folders() {
        let conn = setup_db();
        let html = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<TITLE>Bookmarks</TITLE>
<H1>书签菜单</H1>
<DL><p>
    <DT><A HREF="https://a.com">A</A>
    <DT><A HREF="https://b.com">B</A>
</DL><p>
"#;
        let count = import_from_html(&conn, html).unwrap();
        assert_eq!(count, 2);
        assert_eq!(list_categories(&conn).unwrap().len(), 0);
    }

    #[test]
    fn import_duplicate_url_skipped() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO bookmark (title, url) VALUES (?1, ?2)",
            rusqlite::params!["Existing", "https://dup.com"],
        )
        .unwrap();
        let html = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<H1>Bookmarks</H1>
<DL><p>
    <DT><A HREF="https://dup.com">Duplicate</A>
</DL><p>
"#;
        let count = import_from_html(&conn, html).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn list_bookmarks_ordered_by_sort() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO bookmark (id, title, url, sort_order) VALUES (1, 'A', 'https://a.com', 10)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO bookmark (id, title, url, sort_order) VALUES (2, 'B', 'https://b.com', 5)",
            [],
        )
        .unwrap();
        let list = list_bookmarks_raw(&conn).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].title, "B");
        assert_eq!(list[1].title, "A");
    }

    #[test]
    fn export_as_html_includes_all() {
        let conn = setup_db();
        conn.execute("INSERT INTO category (id, name) VALUES (1, 'Dev')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO bookmark (id, title, url, category_id) VALUES (1, 'GitHub', 'https://github.com', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO bookmark (id, title, url) VALUES (2, 'Root', 'https://example.com')",
            [],
        )
        .unwrap();
        let cats = list_categories(&conn).unwrap();
        let bms = list_bookmarks_raw(&conn).unwrap();
        let html = export_as_html(&cats, &bms);
        assert!(html.contains("GitHub"));
        assert!(html.contains("Root"));
        assert!(html.contains("Dev"));
    }
}
