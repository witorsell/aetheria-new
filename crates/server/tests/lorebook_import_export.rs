mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::multipart_body;
use tower::ServiceExt;

#[tokio::test]
async fn exporting_and_reimporting_a_lorebook_round_trips_its_entries() {
    let (app, cookie) = common::authed_app().await;

    let lorebook_body = serde_json::json!({
        "name": "Whispering Woods",
        "description": "Secrets of the forest.",
        "scan_depth": 3,
        "token_budget": 500,
        "recursive_scanning": true
    })
    .to_string();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/lorebooks")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(lorebook_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let lorebook: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let lorebook_id = lorebook["id"].as_str().unwrap().to_string();

    let entry_body = serde_json::json!({
        "lorebook_id": lorebook_id,
        "name": "The Hollow Oak",
        "entry": "An ancient tree that remembers every promise made beneath it.",
        "keywords": "[\"oak\",\"hollow tree\"]",
        "priority": 42,
        "weight": 7,
        "comment": "Central landmark",
        "constant": true,
        "position": "before_char",
        "probability": 80,
        "use_probability": true,
        "selective": true,
        "selective_logic": 2,
        "exclude_recursion": true
    })
    .to_string();
    let entry_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/lorebooks/{lorebook_id}/entries"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(entry_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(entry_response.status(), StatusCode::OK);

    // export it.
    let export_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/export/lorebook/{lorebook_id}"))
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export_response.status(), StatusCode::OK);
    let export_bytes = axum::body::to_bytes(export_response.into_body(), usize::MAX).await.unwrap();
    let exported: serde_json::Value = serde_json::from_slice(&export_bytes).unwrap();

    assert_eq!(exported["name"], "Whispering Woods");
    assert_eq!(exported["description"], "Secrets of the forest.");
    let exported_entries = exported["entries"].as_array().unwrap();
    assert_eq!(exported_entries.len(), 1);
    let e = &exported_entries[0];
    assert_eq!(e["content"], "An ancient tree that remembers every promise made beneath it.");
    assert_eq!(e["keys"], serde_json::json!(["oak", "hollow tree"]));
    assert_eq!(e["position"], "before_char");
    assert_eq!(e["insertion_order"], 42);
    assert_eq!(e["constant"], true);
    assert_eq!(e["extensions"]["weight"], 7);
    assert_eq!(e["extensions"]["probability"], 80);
    assert_eq!(e["extensions"]["useProbability"], true);
    assert_eq!(e["extensions"]["selectiveLogic"], 2);
    assert_eq!(e["extensions"]["excludeRecursion"], true);

    // re-import the exported JSON as a fresh lorebook.
    let (content_type, body) = multipart_body(
        "file",
        "lorebook.json",
        "application/json",
        exported.to_string().as_bytes(),
    );
    let import_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/import/lorebook")
                .header(header::CONTENT_TYPE, content_type)
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(import_response.status(), StatusCode::OK);
    let import_bytes = axum::body::to_bytes(import_response.into_body(), usize::MAX).await.unwrap();
    let imported_lorebook: serde_json::Value = serde_json::from_slice(&import_bytes).unwrap();
    let imported_id = imported_lorebook["id"].as_str().unwrap().to_string();
    assert_ne!(imported_id, lorebook_id, "import should create a new lorebook, not touch the original");
    assert_eq!(imported_lorebook["name"], "Whispering Woods");

    // there are now two lorebooks, and the imported one carries the same entry.
    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/lorebooks")
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list_bytes = axum::body::to_bytes(list_response.into_body(), usize::MAX).await.unwrap();
    let list: serde_json::Value = serde_json::from_slice(&list_bytes).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 2);

    let entries_response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/lorebooks/{imported_id}/entries"))
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let entries_bytes = axum::body::to_bytes(entries_response.into_body(), usize::MAX).await.unwrap();
    let imported_entries: serde_json::Value = serde_json::from_slice(&entries_bytes).unwrap();
    let imported_entries = imported_entries.as_array().unwrap();
    assert_eq!(imported_entries.len(), 1);
    assert_eq!(
        imported_entries[0]["entry"],
        "An ancient tree that remembers every promise made beneath it."
    );
    assert_eq!(imported_entries[0]["keywords"], "[\"oak\",\"hollow tree\"]");
    assert_eq!(imported_entries[0]["priority"], 42);
    assert_eq!(imported_entries[0]["weight"], 7);
    assert_eq!(imported_entries[0]["selective_logic"], 2);
    assert_eq!(imported_entries[0]["exclude_recursion"], true);
}

#[tokio::test]
async fn importing_a_sillytavern_world_info_object_keeps_its_entries() {
    let (app, cookie) = common::authed_app().await;

    // standalone SillyTavern world info: entries is an object keyed by uid,
    // keywords live on `key`, order on `order`, and position is a number.
    let world = serde_json::json!({
        "name": "Alpha World Info",
        "entries": {
            "7": {
                "uid": 7,
                "key": ["phone", "microwave"],
                "keysecondary": ["lab"],
                "comment": "Phone Microwave",
                "content": "The Phone Microwave sits on the bench.",
                "constant": false,
                "selective": true,
                "selectiveLogic": 1,
                "order": 100,
                "position": 0,
                "disable": false,
                "excludeRecursion": true,
                "probability": 80,
                "useProbability": false
            },
            "2": {
                "uid": 2,
                "key": "okabe, hououin",
                "comment": "",
                "content": "Okabe Rintarou calls himself Hououin Kyouma.",
                "order": 50,
                "position": 1,
                "disable": true
            }
        }
    });
    let (content_type, body) = multipart_body(
        "file",
        "world.json",
        "application/json",
        world.to_string().as_bytes(),
    );
    let import_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/import/lorebook")
                .header(header::CONTENT_TYPE, content_type)
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(import_response.status(), StatusCode::OK);
    let import_bytes = axum::body::to_bytes(import_response.into_body(), usize::MAX).await.unwrap();
    let imported: serde_json::Value = serde_json::from_slice(&import_bytes).unwrap();
    let imported_id = imported["id"].as_str().unwrap();
    assert_eq!(imported["name"], "Alpha World Info");

    let entries_response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/lorebooks/{imported_id}/entries"))
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let entries_bytes = axum::body::to_bytes(entries_response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&entries_bytes).unwrap();
    assert_eq!(entries.len(), 2);

    let phone = entries.iter().find(|e| e["entry"] == "The Phone Microwave sits on the bench.").unwrap();
    assert_eq!(phone["name"], "Phone Microwave");
    assert_eq!(phone["keywords"], "[\"phone\",\"microwave\"]");
    assert_eq!(phone["secondary_keys"], "[\"lab\"]");
    assert_eq!(phone["priority"], 100);
    assert_eq!(phone["enabled"], true);
    assert_eq!(phone["position"], "before_char");
    assert_eq!(phone["probability"], 80);
    assert_eq!(phone["use_probability"], false);
    assert_eq!(phone["selective"], true);
    assert_eq!(phone["selective_logic"], 1);
    assert_eq!(phone["exclude_recursion"], true);
    assert_eq!(phone["constant"], false);

    let okabe = entries.iter().find(|e| e["entry"] == "Okabe Rintarou calls himself Hououin Kyouma.").unwrap();
    assert_eq!(okabe["keywords"], "[\"okabe\",\"hououin\"]");
    assert_eq!(okabe["priority"], 50);
    assert_eq!(okabe["enabled"], false);
    assert_eq!(okabe["position"], "after_char");
    assert_eq!(okabe["probability"], 100);
    assert_eq!(okabe["use_probability"], true);
}
