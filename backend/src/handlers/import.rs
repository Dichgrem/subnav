use crate::error::AppError;
use crate::models::{Bookmark, Category};
use rusqlite::Connection;

/// 提取 `<DT><A ...>文本</A>` 中标签内的文本（最后一个 `>` 到 `</` 之间）。
fn extract_inner_text(line: &str) -> String {
    if let Some(close_start) = line.rfind("</") {
        let before_close = &line[..close_start];
        if let Some(pos) = before_close.rfind('>') {
            return before_close[pos + 1..].to_string();
        }
    }
    String::new()
}

/// 按名称提取属性值（忽略大小写、支持双引号/单引号/无引号，属性顺序无关）。
/// 例如 `href="https://x.com"`、`HREF='https://x.com'`。
fn extract_attr(line: &str, name: &str) -> Option<String> {
    let lower_line = line.to_lowercase();
    let lower_name = name.to_lowercase();
    let needle = format!("{}=", lower_name);

    let mut from = 0;
    while let Some(rel) = lower_line[from..].find(&needle) {
        let val_start = from + rel + needle.len();
        let rest = &line[val_start..];
        let mut chars = rest.chars();
        match chars.next() {
            Some('"') | Some('\'') => {
                let quote = rest.chars().next().unwrap();
                let mut value = String::new();
                for c in rest.chars().skip(1) {
                    if c == quote {
                        return Some(value);
                    }
                    value.push(c);
                }
                return Some(value); // 未闭合引号，取到行尾
            }
            Some(_) => {
                let value: String = chars.take_while(|c| !c.is_whitespace()).collect();
                if !value.is_empty() {
                    return Some(value);
                }
            }
            None => return None,
        }
        from = val_start + 1;
    }
    None
}

/// 解码 HTML 实体（标题/文本中常见的）。
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

pub fn insert_bookmark_if_new(
    conn: &Connection,
    bookmark: &Bookmark,
) -> Result<Option<i64>, AppError> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM bookmark WHERE url = ?1",
            rusqlite::params![bookmark.url],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !exists {
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
        conn.execute(
            "UPDATE bookmark SET sort_order = id WHERE id = ?1 AND sort_order = 0",
            rusqlite::params![id],
        )?;
        Ok(Some(1))
    } else {
        Ok(None)
    }
}

pub fn get_category_by_name(
    conn: &Connection,
    name: &str,
    parent_id: Option<i64>,
) -> Option<Category> {
    conn.query_row(
        "SELECT id, name, parent_id, sort_order FROM category WHERE name = ?1 AND ((?2 IS NULL AND parent_id IS NULL) OR parent_id = ?2)",
        rusqlite::params![name, parent_id],
        |row| {
            Ok(Category {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                parent_id: row.get(2)?,
                sort_order: row.get(3)?,
            })
        },
    )
    .ok()
}

pub fn import_from_html(conn: &Connection, html: &str) -> Result<i64, AppError> {
    let mut stack: Vec<i64> = Vec::new();
    let mut count = 0i64;

    for line in html.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_lowercase();

        if lower.contains("<dt") && lower.contains("<h3") {
            // 文件夹：取 <H3> 文本作为分类名
            let name = decode_entities(&extract_inner_text(trimmed));
            let name = name.trim().to_string();
            let parent_id = stack.last().copied();
            let cat = get_category_by_name(conn, &name, parent_id);
            let cat_id = if let Some(c) = cat {
                c.id.unwrap()
            } else {
                conn.execute(
                    "INSERT INTO category (name, parent_id, sort_order) VALUES (?1, ?2, 0)",
                    rusqlite::params![name, parent_id],
                )?;
                conn.last_insert_rowid()
            };
            stack.push(cat_id);
        } else if lower.contains("</dl>") {
            stack.pop();
        } else if lower.contains("<dt") && lower.contains("<a") {
            // 书签
            let url = extract_attr(trimmed, "href").unwrap_or_default();
            let title = decode_entities(&extract_inner_text(trimmed));
            let title = title.trim().to_string();
            // 优先内嵌的 ICON（base64 data URI），可离线显示；
            // 没有 ICON 才用 ICON_URI（外链，稍后由 fetchIcons 本地化）
            let icon = extract_attr(trimmed, "icon").or_else(|| extract_attr(trimmed, "icon_uri"));

            if !url.is_empty() {
                let category_id = stack.last().copied();
                let bookmark = Bookmark {
                    id: None,
                    title,
                    url,
                    icon,
                    category_id,
                    sort_order: 0,
                };
                if let Some(c) = insert_bookmark_if_new(conn, &bookmark)? {
                    count += c;
                }
            }
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn extract_attr_case_and_order_insensitive() {
        let line =
            r#"<DT><A ADD_DATE="1" HREF="https://a.com" ICON_URI="https://a.com/fav.ico">A</A>"#;
        assert_eq!(extract_attr(line, "href").as_deref(), Some("https://a.com"));
        assert_eq!(
            extract_attr(line, "icon_uri").as_deref(),
            Some("https://a.com/fav.ico")
        );
        // 小写属性
        let lower = r#"<DT><a href='https://b.com'>B</a>"#;
        assert_eq!(
            extract_attr(lower, "href").as_deref(),
            Some("https://b.com")
        );
    }

    #[test]
    fn decode_entities_decodes_common() {
        assert_eq!(decode_entities("A &amp; B"), "A & B");
        assert_eq!(
            decode_entities("&lt;x&gt; &quot;q&quot; &#39;s&#39;"),
            "<x> \"q\" 's'"
        );
    }

    #[test]
    fn import_lowercase_and_entities() {
        let conn = setup_db();
        let html = r#"<dl><p>
<dt><h3>Dev &amp; Tools</h3>
<dl><p>
<dt><a href="https://github.com">Git &lt;Hub&gt;</a>
</dl><p>
</dl><p>"#;
        let count = import_from_html(&conn, html).unwrap();
        assert_eq!(count, 1);
        let folder = get_category_by_name(&conn, "Dev & Tools", None).unwrap();
        assert_eq!(folder.name, "Dev & Tools");
        let bm: (String, Option<i64>) = conn
            .query_row("SELECT title, category_id FROM bookmark", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(bm.0, "Git <Hub>");
        assert_eq!(bm.1, folder.id);
    }

    #[test]
    fn import_prefers_icon_base64() {
        let conn = setup_db();
        let html = r#"<dl><p>
<dt><a href="https://x.com" ICON_URI="https://x.com/fav.ico" ICON="data:image/png;base64,AAAA">X</a>
</dl><p>"#;
        import_from_html(&conn, html).unwrap();
        let icon: Option<String> = conn
            .query_row("SELECT icon FROM bookmark", [], |row| row.get(0))
            .unwrap();
        assert_eq!(icon.as_deref(), Some("data:image/png;base64,AAAA"));
    }
}
