use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn health_returns_ok() {
    std::env::set_var(
        "AETHERIA_SESSION_SECRET",
        "test-secret-at-least-64-bytes-long-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    );
    std::env::set_var("AETHERIA_ENCRYPTION_KEY", "01234567890123456789012345678901"[..32].to_string());

    let db = server::db::connect(":memory:").await;
    let app = server::routes::build_router(server::state::AppState::new(db, true));

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
