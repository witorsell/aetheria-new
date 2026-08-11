use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

async fn test_app() -> axum::Router {
    std::env::set_var(
        "AETHERIA_SESSION_SECRET",
        "test-secret-at-least-64-bytes-long-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    );
    std::env::set_var("AETHERIA_ENCRYPTION_KEY", "01234567890123456789012345678901"[..32].to_string());

    let db = server::db::connect(":memory:").await;
    server::bootstrap_user(&db, "testuser", "test-pass-1234").await;
    server::routes::build_router(server::state::AppState::new(db, true))
}

#[tokio::test]
async fn wrong_password_is_rejected() {
    let app = test_app().await;
    let body = serde_json::json!({"username": "testuser", "password": "wrong"}).to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn repeated_wrong_passwords_lock_the_account_out() {
    let app = test_app().await;
    let wrong_body = serde_json::json!({"username": "testuser", "password": "wrong"}).to_string();

    for _ in 0..5 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(wrong_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    // the 6th attempt is locked out even though the account itself isn't.
    let locked_out = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(wrong_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(locked_out.status(), StatusCode::TOO_MANY_REQUESTS);

    // even the correct password is rejected while locked out.
    let correct_body = serde_json::json!({"username": "testuser", "password": "test-pass-1234"}).to_string();
    let still_locked = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(correct_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(still_locked.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn correct_password_sets_cookie_and_gates_protected_routes() {
    let app = test_app().await;
    let body = serde_json::json!({"username": "testuser", "password": "test-pass-1234"}).to_string();

    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login_response.status(), StatusCode::OK);
    let cookie = login_response
        .headers()
        .get(header::SET_COOKIE)
        .expect("login should set a cookie")
        .to_str()
        .unwrap()
        .to_string();

    // without the cookie, a protected route is rejected.
    let unauthed = app
        .clone()
        .oneshot(Request::builder().uri("/api/characters").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(unauthed.status(), StatusCode::UNAUTHORIZED);

    // with the cookie, it succeeds.
    let authed = app
        .oneshot(
            Request::builder()
                .uri("/api/characters")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authed.status(), StatusCode::OK);
}

#[tokio::test]
async fn lockout_is_case_insensitive() {
    let app = test_app().await;

    for username in ["testuser", "TESTUSER", "Testuser", "tEstUser", "TESTUSEr"] {
        let wrong_body = serde_json::json!({"username": username, "password": "wrong"}).to_string();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(wrong_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let wrong_body = serde_json::json!({"username": "testuser", "password": "wrong"}).to_string();
    let locked_out = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(wrong_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(locked_out.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn successful_login_clears_attempts() {
    let app = test_app().await;
    let wrong_body = serde_json::json!({"username": "testuser", "password": "wrong"}).to_string();
    let correct_body = serde_json::json!({"username": "testuser", "password": "test-pass-1234"}).to_string();

    for _ in 0..3 {
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(wrong_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    let ok_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(correct_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok_resp.status(), StatusCode::OK);

    for _ in 0..3 {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(wrong_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }
}
