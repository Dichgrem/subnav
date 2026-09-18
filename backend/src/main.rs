mod auth;
mod config;
mod db;
mod error;
mod handlers;
mod models;
mod rate_limit;
mod result;

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::Mutex;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::db::DbPool;
use crate::handlers::check::{CheckProgress, CheckResult};
use crate::rate_limit::LoginLimiter;

type CheckCache = Arc<Mutex<HashMap<i64, (i64, Vec<CheckResult>)>>>;

#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub config: Config,
    pub jwt_secret: String,
    pub check_state: Arc<Mutex<HashMap<i64, CheckProgress>>>,
    pub check_cache: CheckCache,
    pub login_limiter: LoginLimiter,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = config::Config::from_env();
    let (pool, jwt_secret) = db::init_db(&config.database_path);
    db::ensure_admin_user(&pool, &config.admin_username, &config.admin_password);

    let state = AppState {
        db: pool,
        config: config.clone(),
        jwt_secret,
        check_state: Arc::new(Mutex::new(HashMap::new())),
        check_cache: Arc::new(Mutex::new(HashMap::new())),
        login_limiter: LoginLimiter::default(),
    };

    let auth_routes = Router::new()
        .route("/user/login", post(handlers::user::login))
        .route(
            "/user/changePassword",
            post(handlers::user::change_password),
        );

    let api_routes = Router::new()
        .route("/category/list", get(handlers::category::list))
        .route("/category/add", post(handlers::category::add))
        .route(
            "/category/batchUpdate",
            post(handlers::category::batch_update),
        )
        .route("/category/delete", post(handlers::category::delete))
        .route("/bookmark/list", get(handlers::bookmark::list))
        .route("/bookmark/add", post(handlers::bookmark::add))
        .route("/bookmark/delete", post(handlers::bookmark::delete))
        .route(
            "/bookmark/batchUpdateSort",
            post(handlers::bookmark::batch_update_sort),
        )
        .route(
            "/bookmark/export",
            get(handlers::bookmark::export_bookmarks),
        )
        .route(
            "/bookmark/import",
            post(handlers::bookmark::import_bookmarks),
        )
        .route(
            "/bookmark/fetchIcons",
            post(handlers::bookmark::fetch_icons),
        )
        .route("/bookmark/checkLinks", post(handlers::check::check_links))
        .route(
            "/bookmark/checkLinks/status",
            get(handlers::check::check_status),
        );

    let cors = CorsLayer::new()
        .allow_origin(
            config
                .cors_origin
                .parse::<axum::http::HeaderValue>()
                .expect("Invalid CORS origin"),
        )
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
        ]);

    let app = Router::new()
        .merge(auth_routes)
        .merge(api_routes)
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)))
        .with_state(state)
        .fallback_service(ServeDir::new(&config.frontend_dir))
        // 必须放在 fallback_service 之后：Router::layer 只作用于调用时已存在的路由，
        // 否则静态资源（js/css/html）不会被压缩。
        .layer(CompressionLayer::new());

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    tracing::info!("Server running on http://{}", addr);

    axum::serve(listener, app).await.expect("Server error");
}
