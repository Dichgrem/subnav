use crate::models::{Bookmark, Category};
use std::collections::HashMap;

pub fn export_as_html(categories: &[Category], bookmarks: &[Bookmark]) -> String {
    let mut children_map: HashMap<i64, Vec<&Category>> = HashMap::new();
    let mut bookmark_map: HashMap<i64, Vec<&Bookmark>> = HashMap::new();

    for cat in categories {
        if let Some(parent_id) = cat.parent_id {
            children_map.entry(parent_id).or_default().push(cat);
        }
    }
    for bm in bookmarks {
        if let Some(cat_id) = bm.category_id {
            bookmark_map.entry(cat_id).or_default().push(bm);
        }
    }

    let mut sb = String::new();
    sb.push_str("<!DOCTYPE NETSCAPE-Bookmark-file-1>\n");
    sb.push_str(
        "<!-- This is an automatically generated file.\n     It will be read and overwritten.\n     DO NOT EDIT! -->\n",
    );
    sb.push_str("<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n");
    sb.push_str("<TITLE>Bookmarks</TITLE>\n");
    sb.push_str("<H1>书签菜单</H1>\n");
    sb.push_str("<DL><p>\n");

    for cat in categories {
        if cat.parent_id.is_none() {
            build_category_html(&mut sb, cat, &children_map, &bookmark_map, 1);
        }
    }
    for bm in bookmarks {
        if bm.category_id.is_none() {
            append_indent(&mut sb, 1);
            format_bookmark_html(&mut sb, bm);
        }
    }

    sb.push_str("</DL><p>\n");
    sb
}

fn build_category_html(
    sb: &mut String,
    category: &Category,
    children_map: &HashMap<i64, Vec<&Category>>,
    bookmark_map: &HashMap<i64, Vec<&Bookmark>>,
    depth: i32,
) {
    append_indent(sb, depth);
    sb.push_str(&format!("<DT><H3>{}</H3>\n", escape_html(&category.name)));
    append_indent(sb, depth);
    sb.push_str("<DL><p>\n");

    if let Some(id) = category.id {
        if let Some(cat_bookmarks) = bookmark_map.get(&id) {
            for bm in cat_bookmarks {
                append_indent(sb, depth + 1);
                format_bookmark_html(sb, bm);
            }
        }
    }
    if let Some(id) = category.id {
        if let Some(children) = children_map.get(&id) {
            for child in children {
                build_category_html(sb, child, children_map, bookmark_map, depth + 1);
            }
        }
    }

    append_indent(sb, depth);
    sb.push_str("</DL><p>\n");
}

fn append_indent(sb: &mut String, depth: i32) {
    for _ in 0..depth {
        sb.push_str("    ");
    }
}

pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn format_bookmark_html(sb: &mut String, bm: &Bookmark) {
    if let Some(ref icon) = bm.icon {
        sb.push_str(&format!(
            "<DT><A HREF=\"{}\" ICON=\"{}\">{}</A>\n",
            escape_html(&bm.url),
            escape_html(icon),
            escape_html(&bm.title)
        ));
    } else {
        sb.push_str(&format!(
            "<DT><A HREF=\"{}\">{}</A>\n",
            escape_html(&bm.url),
            escape_html(&bm.title)
        ));
    }
}
