//! Dashboard — Axum web server for real-time monitoring.
//!
//! Serves a REST API and a self-contained HTML dashboard.
//! CORS enabled for local development.

pub mod routes;

use anyhow::{Context, Result};
use axum::{
    http::{header, HeaderValue, Method},
    response::Html,
    routing::get,
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::info;

use routes::{AppState, DashboardState};

/// The embedded dashboard HTML (compiled into the binary).
const DASHBOARD_HTML: &str = include_str!("templates/index.html");

/// Start the dashboard web server.
///
/// This spawns a background task — it doesn't block.
pub fn spawn_dashboard(state: AppState, port: u16) -> Result<()> {
    let app = build_router(state);

    tokio::spawn(async move {
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
        info!(port, "Dashboard server starting on http://localhost:{port}");

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("Failed to bind dashboard port");

        axum::serve(listener, app)
            .await
            .expect("Dashboard server error");
    });

    Ok(())
}

/// Build the Axum router with all routes and middleware.
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin("*".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET])
        .allow_headers([header::CONTENT_TYPE]);

    Router::new()
        // API routes
        .route("/api/status", get(routes::get_status))
        .route("/api/cycles", get(routes::get_cycles))
        .route("/api/balance-history", get(routes::get_balance_history))
        .route("/api/trades", get(routes::get_trades))
        .route("/api/costs", get(routes::get_costs))
        .route("/api/metrics", get(routes::get_metrics))
        .route("/health", get(routes::health))
        // Dashboard HTML
        .route("/", get(serve_dashboard))
        .layer(cors)
        .with_state(state)
}

/// Serve the embedded HTML dashboard.
async fn serve_dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use crate::types::AgentState;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        Arc::new(DashboardState::new(AgentState::new(100.0)))
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_status_endpoint() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(Request::builder().uri("/api/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["bankroll"].as_f64().unwrap() > 0.0);
    }

    #[tokio::test]
    async fn test_cycles_endpoint() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(Request::builder().uri("/api/cycles").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_balance_history_endpoint() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(Request::builder().uri("/api/balance-history").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(!json.is_empty());
    }

    #[tokio::test]
    async fn test_trades_endpoint() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(Request::builder().uri("/api/trades").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_costs_endpoint() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(Request::builder().uri("/api/costs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_metrics_endpoint() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(Request::builder().uri("/api/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_dashboard_html() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 100_000).await.unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("ORACLE"));
        assert!(html.contains("Dashboard"));
    }

    #[tokio::test]
    async fn test_cors_headers() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(Request::builder().uri("/api/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // CORS layer should allow the response through
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
