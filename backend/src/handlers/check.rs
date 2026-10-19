use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::State;
use reqwest::Client;
use serde::Serialize;
use tokio::sync::Semaphore;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::result::ApiResult;
use crate::AppState;

const CONCURRENCY: usize = 20;
const REQUEST_TIMEOUT_SECS: u64 = 3;
const CACHE_TTL_SECS: i64 = 86400;

#[derive(Clone, Debug)]
pub struct CheckProgress {
    pub total: usize,
    pub completed: usize,
    pub results: Vec<CheckResult>,
    pub finished: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    pub level: String,
    pub http_code: Option<u16>,
    pub error: Option<String>,
    pub base_url: String,
    pub base_alive: bool,
    pub final_url: Option<String>,
    pub checked_at: i64,
    pub cached: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckStatusResponse {
    total: usize,
    completed: usize,
    finished: bool,
    results: Option<Vec<CheckResult>>,
}

pub async fn check_links(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<ApiResult<String>, AppError> {
    let user_id = auth.0;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let cached = state.check_cache.lock().await.get(&user_id).cloned();

    if let Some((cached_at, cached_results)) = cached {
        if now - cached_at < CACHE_TTL_SECS {
            let mut state_map = state.check_state.lock().await;
            state_map.insert(
                user_id,
                CheckProgress {
                    total: cached_results.len(),
                    completed: cached_results.len(),
                    results: cached_results.clone(),
                    finished: true,
                },
            );
            return Ok(ApiResult::success("cached".to_string()));
        }
    }

    {
        let mut state_map = state.check_state.lock().await;
        if state_map.contains_key(&user_id) {
            return Ok(ApiResult::error("检测正在进行中"));
        }
        state_map.insert(
            user_id,
            CheckProgress {
                total: 0,
                completed: 0,
                results: vec![],
                finished: false,
            },
        );
    }

    let state_clone = state.clone();
    tokio::spawn(async move {
        let results = check_all_links(state_clone.clone(), user_id).await;
        state_clone
            .check_cache
            .lock()
            .await
            .insert(user_id, (now, results));
    });

    Ok(ApiResult::success("ok".to_string()))
}

pub async fn check_status(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<ApiResult<CheckStatusResponse>, AppError> {
    let mut state_map = state.check_state.lock().await;
    match state_map.get(&auth.0) {
        Some(progress) => {
            let finished = progress.finished;
            let response = CheckStatusResponse {
                total: progress.total,
                completed: progress.completed,
                finished,
                results: if finished {
                    Some(progress.results.clone())
                } else {
                    None
                },
            };
            if finished {
                state_map.remove(&auth.0);
            }
            Ok(ApiResult::success(response))
        }
        None => Ok(ApiResult::success(CheckStatusResponse {
            total: 0,
            completed: 0,
            finished: false,
            results: None,
        })),
    }
}

async fn check_all_links(state: AppState, user_id: i64) -> Vec<CheckResult> {
    let allow_private = state.config.allow_private_urls;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let bookmarks = load_bookmarks(&state);
    if bookmarks.is_empty() {
        let mut state_map = state.check_state.lock().await;
        if let Some(progress) = state_map.get_mut(&user_id) {
            progress.finished = true;
        }
        return vec![];
    }

    let total = bookmarks.len();

    {
        let mut state_map = state.check_state.lock().await;
        if let Some(progress) = state_map.get_mut(&user_id) {
            progress.total = total;
        }
    }

    let client = match Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .user_agent(format!("subnav/{}", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::limited(5))
        .tcp_keepalive(Duration::from_secs(30))
        .pool_max_idle_per_host(4)
        .build()
    {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let semaphore = Arc::new(Semaphore::new(CONCURRENCY));
    let mut handles = Vec::with_capacity(total);

    for (id, title, url, icon) in bookmarks {
        let client = client.clone();
        let sem = semaphore.clone();
        let state = state.clone();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let result =
                check_single_link(&client, id, &title, &url, icon, now, allow_private).await;

            let mut state_map = state.check_state.lock().await;
            if let Some(progress) = state_map.get_mut(&user_id) {
                progress.results.push(result);
                progress.completed += 1;
            }
        }));
    }

    for h in handles {
        if let Err(e) = h.await {
            tracing::error!("check task failed: {e:?}");
        }
    }

    let mut state_map = state.check_state.lock().await;
    match state_map.get_mut(&user_id) {
        Some(progress) => {
            progress.finished = true;
            progress.results.sort_by_key(|r| r.id);
            progress.results.clone()
        }
        None => vec![],
    }
}

fn load_bookmarks(state: &AppState) -> Vec<(i64, String, String, Option<String>)> {
    let conn = match state.db.get() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut stmt = match conn.prepare("SELECT id, title, url, icon FROM bookmark") {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    });
    match rows {
        Ok(r) => r.filter_map(|r| r.ok()).collect(),
        Err(_) => vec![],
    }
}

async fn check_single_link(
    client: &Client,
    id: i64,
    title: &str,
    url_str: &str,
    icon: Option<String>,
    now: i64,
    allow_private: bool,
) -> CheckResult {
    let base_url = extract_base_origin(url_str);

    let (level, http_code, error, final_url) = match probe_url(client, url_str, allow_private).await
    {
        ProbeResult::Ok { status, final_url } => ("ok".to_string(), Some(status), None, final_url),
        ProbeResult::Suspect {
            status, final_url, ..
        } => (
            "suspect".to_string(),
            Some(status),
            Some("redirected to external domain".to_string()),
            final_url,
        ),
        ProbeResult::Dead {
            status,
            error_msg,
            final_url,
        } => {
            let base_alive = probe_base_alive(client, &base_url, allow_private).await;
            if base_alive {
                ("page_dead".to_string(), status, Some(error_msg), final_url)
            } else {
                ("site_dead".to_string(), status, Some(error_msg), final_url)
            }
        }
    };

    let base_alive = level != "site_dead" && !base_url.is_empty();

    CheckResult {
        id,
        title: title.to_string(),
        url: url_str.to_string(),
        icon,
        level,
        http_code,
        error,
        base_url,
        base_alive,
        final_url,
        checked_at: now,
        cached: false,
    }
}

enum ProbeResult {
    Ok {
        status: u16,
        final_url: Option<String>,
    },
    Suspect {
        status: u16,
        final_url: Option<String>,
    },
    Dead {
        status: Option<u16>,
        error_msg: String,
        final_url: Option<String>,
    },
}

async fn probe_url(client: &Client, url_str: &str, allow_private: bool) -> ProbeResult {
    if is_blocked_url(url_str, allow_private).await {
        return ProbeResult::Dead {
            status: None,
            error_msg: "blocked: private or local address".to_string(),
            final_url: None,
        };
    }
    match client.head(url_str).send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let final_url = Some(resp.url().to_string());
            if status < 400 {
                if is_cross_domain(url_str, resp.url().as_str()) {
                    return ProbeResult::Suspect { status, final_url };
                }
                return ProbeResult::Ok { status, final_url };
            }
            ProbeResult::Dead {
                status: Some(status),
                error_msg: format!("HTTP {}", status),
                final_url,
            }
        }
        Err(_) => match client.get(url_str).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let final_url = Some(resp.url().to_string());
                if is_cross_domain(url_str, resp.url().as_str()) {
                    return ProbeResult::Suspect { status, final_url };
                }
                if status < 400 {
                    return ProbeResult::Ok { status, final_url };
                }
                ProbeResult::Dead {
                    status: Some(status),
                    error_msg: format!("HTTP {}", status),
                    final_url,
                }
            }
            Err(e) => ProbeResult::Dead {
                status: None,
                error_msg: e.to_string(),
                final_url: None,
            },
        },
    }
}

fn is_cross_domain(original: &str, final_url: &str) -> bool {
    let orig = match url::Url::parse(original) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let dest = match url::Url::parse(final_url) {
        Ok(u) => u,
        Err(_) => return false,
    };
    orig.host_str() != dest.host_str()
}

async fn probe_base_alive(client: &Client, base_url: &str, allow_private: bool) -> bool {
    if base_url.is_empty() || is_blocked_url(base_url, allow_private).await {
        return false;
    }
    match client.head(base_url).send().await {
        Ok(resp) => resp.status().as_u16() < 400,
        Err(_) => match client.get(base_url).send().await {
            Ok(resp) => resp.status().as_u16() < 400,
            Err(_) => false,
        },
    }
}

fn extract_base_origin(url_str: &str) -> String {
    match url::Url::parse(url_str) {
        Ok(u) => {
            let host = u.host_str().unwrap_or("");
            if host.is_empty() {
                return String::new();
            }
            if let Some(port) = u.port() {
                format!("{}://{}:{}", u.scheme(), host, port)
            } else {
                format!("{}://{}", u.scheme(), host)
            }
        }
        Err(_) => String::new(),
    }
}

fn is_private_or_local(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0b1100_0000) == 64)
                || (v4.octets()[0] == 192 && v4.octets()[1] == 0 && v4.octets()[2] == 0)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
        }
    }
}

pub(crate) async fn is_blocked_url(url_str: &str, allow_private: bool) -> bool {
    if allow_private {
        return false;
    }
    let parsed = match url::Url::parse(url_str) {
        Ok(u) => u,
        Err(_) => return true,
    };
    let host = match parsed.host_str() {
        Some(h) => h.to_string(),
        None => return true,
    };
    let port = parsed.port_or_known_default().unwrap_or(80);

    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_private_or_local(&ip);
    }

    let blocked = match tokio::net::lookup_host((host.as_str(), port)).await {
        Ok(addrs) => {
            let ips: Vec<IpAddr> = addrs.map(|a| a.ip()).collect();
            ips.is_empty() || ips.iter().any(is_private_or_local)
        }
        Err(_) => true,
    };
    blocked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_private_and_loopback_ips() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
        ] {
            assert!(
                is_private_or_local(&ip.parse::<IpAddr>().unwrap()),
                "should block {ip}"
            );
        }
    }

    #[test]
    fn allows_public_ips() {
        for ip in ["8.8.8.8", "1.1.1.1", "93.184.216.34"] {
            assert!(
                !is_private_or_local(&ip.parse::<IpAddr>().unwrap()),
                "should allow {ip}"
            );
        }
    }
}
