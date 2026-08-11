mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn exported_character_includes_talkativeness_in_extensions() {
    let (app, cookie) = common::authed_app().await;

    let create_body = serde_json::json!({ "name": "Chatty", "talkativeness": 0.8 }).to_string();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/characters")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let exported = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/export/character/{id}"))
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(exported.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(exported.into_body(), usize::MAX).await.unwrap();
    let card: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(card["data"]["extensions"]["talkativeness"], 0.8);
}

#[tokio::test]
async fn import_tavern_v1_b64_png_character_card_succeeds() {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let (app, cookie) = common::authed_app().await;

    let v1_json = serde_json::json!({
        "name": "Peni Parker",
        "description": "Spider-Hero from Earth-14512",
        "first_mes": "Hey there!",
        "scenario": "workshop",
        "mes_example": "<START>"
    }).to_string();

    let b64_payload = STANDARD.encode(v1_json.as_bytes());

    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.add_text_chunk("Chara".to_string(), b64_payload).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0, 0, 0]).unwrap();
    }

    let boundary = "---------------------------1234567890";
    let mut multipart_body = Vec::new();
    multipart_body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"peni.png\"\r\nContent-Type: image/png\r\n\r\n").as_bytes());
    multipart_body.extend_from_slice(&png_bytes);
    multipart_body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/import/character")
                .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
                .header(header::COOKIE, cookie)
                .body(Body::from(multipart_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let card: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(card["name"], "Peni Parker");
    assert_eq!(card["description"], "Spider-Hero from Earth-14512");
}

