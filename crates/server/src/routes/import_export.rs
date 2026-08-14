use axum::{
    extract::Query,
    extract::{Extension, Path, State, Multipart},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use crate::state::AppState;
use serde_json::Value;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::io::Cursor;
use png::Decoder;

pub async fn import_character(
    Extension(_user_id): Extension<i64>,
    State(_state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut file_bytes = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return (StatusCode::BAD_REQUEST, format!("Upload failed: {e}")).into_response(),
        };
        if field.name() == Some("file") {
            match field.bytes().await {
                Ok(data) => {
                    file_bytes = Some(data);
                    break;
                }
                Err(e) => return (StatusCode::BAD_REQUEST, format!("Upload failed: {e}")).into_response(),
            }
        }
    }

    let bytes = match file_bytes {
        Some(b) => b,
        None => return (StatusCode::BAD_REQUEST, "No file uploaded").into_response(),
    };
    let chunks = extract_all_text_chunks_from_png(&bytes);
    let mut selected_chunk_text = None;

    // 1st priority: ccv3 chunk
    for chunk in &chunks {
        if chunk.keyword.eq_ignore_ascii_case("ccv3") {
            selected_chunk_text = Some(chunk.text.clone());
            break;
        }
    }

    // 2nd priority: chara, character, chara_v2, tavern, pygmalion
    if selected_chunk_text.is_none() {
        for chunk in &chunks {
            let kw = chunk.keyword.to_ascii_lowercase();
            if kw == "chara" || kw == "character" || kw == "chara_v2" || kw == "tavern" || kw == "pygmalion" {
                selected_chunk_text = Some(chunk.text.clone());
                break;
            }
        }
    }

    // 3rd priority: any text chunk that decodes to valid json payload
    if selected_chunk_text.is_none() {
        for chunk in &chunks {
            if decode_chara_payload(&chunk.text).is_some() {
                selected_chunk_text = Some(chunk.text.clone());
                break;
            }
        }
    }

    // 4th priority: first text chunk fallback
    if selected_chunk_text.is_none() {
        if let Some(first) = chunks.first() {
            selected_chunk_text = Some(first.text.clone());
        }
    }

    let json = if let Some(t) = selected_chunk_text {
        decode_chara_payload(&t)
    } else {
        None
    };

    let json = match json.or_else(|| find_chara_payload_in_raw_bytes(&bytes)) {
        Some(j) => j,
        None => return (StatusCode::BAD_REQUEST, "No character card metadata or text chunk found in file").into_response(),
    };
    
    Json(json).into_response()
}

struct TextChunkItem {
    keyword: String,
    text: String,
}

fn extract_all_text_chunks_from_png(bytes: &[u8]) -> Vec<TextChunkItem> {
    let mut chunks = Vec::new();

    // 1. png decoder for tEXt, iTXt (utf8_text), zTXt (compressed_latin1_text)
    let decoder = Decoder::new(Cursor::new(bytes));
    if let Ok(mut reader) = decoder.read_info() {
        let buf_size = reader.output_buffer_size().unwrap_or(1024 * 1024);
        let mut buf = vec![0; buf_size];
        while let Ok(_) = reader.next_frame(&mut buf) {}
        let info = reader.info();
        for chunk in &info.uncompressed_latin1_text {
            chunks.push(TextChunkItem {
                keyword: chunk.keyword.clone(),
                text: chunk.text.clone(),
            });
        }
        for chunk in &info.utf8_text {
            if let Ok(t) = chunk.get_text() {
                chunks.push(TextChunkItem {
                    keyword: chunk.keyword.clone(),
                    text: t,
                });
            }
        }
        for chunk in &info.compressed_latin1_text {
            if let Ok(text) = chunk.get_text() {
                chunks.push(TextChunkItem {
                    keyword: chunk.keyword.clone(),
                    text,
                });
            }
        }
    }

    // 2. raw binary scan of PNG chunks
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut pos = 8;
        while pos + 8 <= bytes.len() {
            let len = u32::from_be_bytes(bytes[pos..pos+4].try_into().unwrap_or_default()) as usize;
            let chunk_type = &bytes[pos+4..pos+8];
            pos += 8;

            if pos + len > bytes.len() {
                break;
            }

            let chunk_data = &bytes[pos..pos+len];
            pos += len + 4;

            if chunk_type == b"tEXt" || chunk_type == b"zTXt" || chunk_type == b"iTXt" {
                if let Some(null_idx) = chunk_data.iter().position(|&b| b == 0) {
                    let kw = String::from_utf8_lossy(&chunk_data[..null_idx]).to_string();
                    let payload_bytes = &chunk_data[null_idx+1..];

                    if chunk_type == b"tEXt" {
                        let text = String::from_utf8_lossy(payload_bytes).to_string();
                        if !chunks.iter().any(|c| c.keyword == kw && c.text == text) {
                            chunks.push(TextChunkItem { keyword: kw, text });
                        }
                    } else if chunk_type == b"iTXt" {
                        if payload_bytes.len() >= 2 {
                            let comp_flag = payload_bytes[0];
                            let rest = &payload_bytes[2..];
                            let mut p = 0;
                            let mut nulls = 0;
                            while p < rest.len() && nulls < 2 {
                                if rest[p] == 0 {
                                    nulls += 1;
                                }
                                p += 1;
                            }
                            if p <= rest.len() {
                                let text_bytes = &rest[p..];
                                if comp_flag == 0 {
                                    let text = String::from_utf8_lossy(text_bytes).to_string();
                                    if !chunks.iter().any(|c| c.keyword == kw && c.text == text) {
                                        chunks.push(TextChunkItem { keyword: kw, text });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    chunks
}

fn find_chara_payload_in_raw_bytes(bytes: &[u8]) -> Option<Value> {
    let ascii_str = String::from_utf8_lossy(bytes);
    
    for chunk in ascii_str.split(|c: char| !c.is_alphanumeric() && c != '+' && c != '/' && c != '=' && c != '-' && c != '_') {
        let trimmed = chunk.trim();
        if trimmed.len() >= 20 {
            if let Some(val) = decode_chara_payload(trimmed) {
                return Some(val);
            }
        }
    }

    let mut start_idx = None;
    let mut depth = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'{' {
            if depth == 0 {
                start_idx = Some(i);
            }
            depth += 1;
        } else if b == b'}' && depth > 0 {
            depth -= 1;
            if depth == 0 {
                if let Some(s) = start_idx {
                    let slice = &bytes[s..=i];
                    if let Ok(val) = serde_json::from_slice::<Value>(slice) {
                        if val.get("name").is_some() || val.get("ch_name").is_some() || val.get("char_name").is_some() || val.get("data").is_some() {
                            return Some(val);
                        }
                    }
                }
            }
        }
    }

    None
}

fn decode_chara_payload(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. raw JSON directly
    if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
        return Some(val);
    }

    // 2. base64 decoders (standard, url-safe, padded and unpadded)
    let engines = [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];

    for engine in engines {
        if let Ok(bytes) = engine.decode(trimmed) {
            if let Ok(val) = serde_json::from_slice::<Value>(&bytes) {
                return Some(val);
            }
            if let Ok(s) = std::str::from_utf8(&bytes) {
                if let Ok(val) = serde_json::from_str::<Value>(s.trim()) {
                    return Some(val);
                }
            }
        }
    }

    None
}


#[derive(serde::Deserialize)]
pub struct ExportQuery {
    format: Option<String>,
}

use serde_json::json;
use std::path::PathBuf;

pub async fn export_character(
    Extension(user_id): Extension<i64>,
    Path(id): Path<String>,
    Query(query): Query<ExportQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let character = match crate::models::character::get(&state.db.read_pool, user_id, &id).await {
        Ok(Some(c)) => c,
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, "Character not found").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to fetch character for export");
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    let mut extensions: serde_json::Value = serde_json::from_str(&character.extensions).unwrap_or(json!({}));
    if let Some(obj) = extensions.as_object_mut() {
        obj.insert("talkativeness".to_string(), json!(character.talkativeness));
    }

    let greetings: Vec<String> = crate::models::character::list_alternate_greetings(&state.db.read_pool, user_id, &id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|g| g.greeting)
        .collect();

    let tag_ids = crate::models::character::list_character_tags(&state.db.read_pool, user_id, &id)
        .await
        .unwrap_or_default();
    let tag_names: Vec<String> = if tag_ids.is_empty() {
        Vec::new()
    } else {
        crate::models::character::list_tags(&state.db.read_pool, user_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|t| tag_ids.contains(&t.id))
            .map(|t| t.name)
            .collect()
    };

    let v2_json = json!({
        "spec": "chara_card_v2",
        "spec_version": "2.0",
        "data": {
            "name": character.name,
            "description": character.description,
            "personality": character.personality,
            "scenario": character.scenario,
            "first_mes": character.first_message,
            "mes_example": character.sample_chat,
            "creator_notes": "",
            "system_prompt": character.system_prompt,
            "post_history_instructions": character.post_history_instructions,
            "tags": tag_names,
            "creator": "",
            "character_version": "",
            "alternate_greetings": greetings,
            "extensions": extensions
        }
    });

    let is_png = query.format.as_deref().unwrap_or("json") == "png";

    if is_png {
        let mut loaded_img = None;

        if let Some(p) = &character.avatar_path {
            if !p.is_empty() {
                loaded_img = image::open(p).ok();
            }
        }

        if loaded_img.is_none() {
            if let Some(u) = &character.avatar_url {
                if u.starts_with("/uploads/") {
                    // avatar_url carries a `?v=` cache-busting query string
                    // (see characters::upload_avatar), which isn't part of
                    // the actual file on disk.
                    let local_path = u.split('?').next().unwrap_or(u).trim_start_matches('/');
                    loaded_img = image::open(local_path).ok();
                } else if u.starts_with("http") {
                    // reuse the same SSRF protections as the image proxy
                    if let Ok(resp) = crate::routes::proxy::proxy_fetch_with_checks(u).await {
                        if let Ok(bytes) = resp.bytes().await {
                            loaded_img = image::load_from_memory(&bytes).ok();
                        }
                    }
                }
            }
        }

        let (width, height, buf) = if let Some(img) = loaded_img {
            let rgba = img.to_rgba8();
            (rgba.width(), rgba.height(), rgba.into_raw())
        } else {
            (1, 1, vec![0, 0, 0, 0])
        };

        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let json_str = serde_json::to_string(&v2_json).unwrap_or_default();
            let b64 = base64::engine::general_purpose::STANDARD.encode(json_str);
            let _ = encoder.add_text_chunk("chara".to_string(), b64);
            if let Ok(mut writer) = encoder.write_header() {
                let _ = writer.write_image_data(&buf);
            }
        }
        return (
            [(
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}.png\"", character.name.replace(" ", "_")),
            ),
            (
                axum::http::header::CONTENT_TYPE,
                "image/png".to_string(),
            )],
            out,
        ).into_response();
    }

    (
        [(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}.json\"", character.name.replace(" ", "_")),
        ),
        (
            axum::http::header::CONTENT_TYPE,
            "application/json".to_string(),
        )],
        Json(v2_json).to_string(),
    ).into_response()
}
/// import/export target the character-book v2 shape (the same interchange
/// format character cards embed their world info under), so a lorebook
/// exported here can be dropped into SillyTavern and vice versa. fields
/// with no home in that spec (weight, probability, useProbability,
/// selectiveLogic, excludeRecursion) live under each entry's `extensions`,
/// matching how SillyTavern itself round-trips its own extra fields there.
pub async fn import_lorebook(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut file_bytes = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return (StatusCode::BAD_REQUEST, format!("Upload failed: {e}")).into_response(),
        };
        if field.name() == Some("file") {
            match field.bytes().await {
                Ok(data) => {
                    file_bytes = Some(data);
                    break;
                }
                Err(e) => return (StatusCode::BAD_REQUEST, format!("Upload failed: {e}")).into_response(),
            }
        }
    }

    let bytes = match file_bytes {
        Some(b) => b,
        None => return (StatusCode::BAD_REQUEST, "No file uploaded").into_response(),
    };

    let book: Value = match serde_json::from_slice(&bytes) {
        Ok(j) => j,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response(),
    };

    let name = book.get("name").and_then(|v| v.as_str()).unwrap_or("Imported Lorebook").to_string();
    let lorebook_input = crate::models::lorebook::CreateLorebookInput {
        user_id: Some(user_id),
        name,
        description: book.get("description").and_then(|v| v.as_str()).map(str::to_string),
        scan_depth: book.get("scan_depth").and_then(|v| v.as_i64()),
        token_budget: book.get("token_budget").and_then(|v| v.as_i64()),
        recursive_scanning: book.get("recursive_scanning").and_then(|v| v.as_bool()),
        extensions: book.get("extensions").map(|v| v.to_string()),
    };

    let lorebook = match state.db.writer.create_lorebook(user_id, lorebook_input).await {
        Ok(l) => l,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create lorebook").into_response(),
    };

    let no_entries = Vec::new();
    let entries = book.get("entries").and_then(|v| v.as_array()).unwrap_or(&no_entries);

    for entry in entries {
        let ext = entry.get("extensions").cloned().unwrap_or(json!({}));
        let keys = entry.get("keys").cloned().unwrap_or(json!([]));
        let secondary_keys = entry.get("secondary_keys").cloned().unwrap_or(json!([]));

        let entry_input = crate::models::lorebook::CreateLorebookEntryInput {
            user_id: Some(user_id),
            lorebook_id: lorebook.id.clone(),
            name: entry.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            entry: entry.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            keywords: Some(keys.to_string()),
            priority: entry.get("insertion_order").and_then(|v| v.as_i64()),
            weight: ext.get("weight").and_then(|v| v.as_i64()),
            enabled: entry.get("enabled").and_then(|v| v.as_bool()),
            comment: entry.get("comment").and_then(|v| v.as_str()).map(str::to_string),
            secondary_keys: Some(secondary_keys.to_string()),
            constant: entry.get("constant").and_then(|v| v.as_bool()),
            position: entry.get("position").and_then(|v| v.as_str()).map(str::to_string),
            // SillyTavern's own real defaults (see its `convertCharacterBook`
            // in world-info.js) when these are absent entirely: probability
            // 100, useProbability true. sending our own None here would let
            // the DB's unwrap_or(0)/unwrap_or(false) apply instead, silently
            // turning "no probability info" into "never activates".
            probability: Some(ext.get("probability").and_then(|v| v.as_i64()).unwrap_or(100)),
            use_probability: Some(ext.get("useProbability").and_then(|v| v.as_bool()).unwrap_or(true)),
            selective: entry.get("selective").and_then(|v| v.as_bool()),
            selective_logic: ext.get("selectiveLogic").and_then(|v| v.as_i64()),
            // our own export writes this back out as "excludeRecursion" (see
            // export_lorebook below) for a clean round trip, but real
            // SillyTavern-authored cards use snake_case "exclude_recursion"
            // (confirmed against SillyTavern's own source), check both.
            exclude_recursion: ext
                .get("excludeRecursion")
                .or_else(|| ext.get("exclude_recursion"))
                .and_then(|v| v.as_bool()),
        };

        let _ = state.db.writer.create_lorebook_entry(user_id, entry_input).await;
    }

    Json(lorebook).into_response()
}

pub async fn export_lorebook(
    Extension(user_id): Extension<i64>,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let lorebook = match crate::models::lorebook::get(&state.db.read_pool, user_id, &id).await {
        Ok(Some(l)) => l,
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, "Lorebook not found").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to fetch lorebook for export");
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };
    let entries = crate::models::lorebook::list_entries(&state.db.read_pool, user_id, &id)
        .await
        .unwrap_or_default();

    let entries_json: Vec<Value> = entries
        .into_iter()
        .map(|e| {
            let keys: Value = serde_json::from_str(&e.keywords).unwrap_or(json!([]));
            let secondary_keys: Value = serde_json::from_str(&e.secondary_keys).unwrap_or(json!([]));
            json!({
                "keys": keys,
                "secondary_keys": secondary_keys,
                "name": e.name,
                "comment": e.comment,
                "content": e.entry,
                "constant": e.constant,
                "selective": e.selective,
                "insertion_order": e.priority,
                "enabled": e.enabled,
                "position": e.position,
                "extensions": {
                    "weight": e.weight,
                    "probability": e.probability,
                    "useProbability": e.use_probability,
                    "selectiveLogic": e.selective_logic,
                    "excludeRecursion": e.exclude_recursion,
                }
            })
        })
        .collect();

    let book = json!({
        "name": lorebook.name,
        "description": lorebook.description,
        "scan_depth": lorebook.scan_depth,
        "token_budget": lorebook.token_budget,
        "recursive_scanning": lorebook.recursive_scanning,
        "extensions": serde_json::from_str::<Value>(&lorebook.extensions).unwrap_or(json!({})),
        "entries": entries_json,
    });

    (
        [(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}.json\"", lorebook.name.replace(" ", "_")),
        ),
        (
            axum::http::header::CONTENT_TYPE,
            "application/json".to_string(),
        )],
        Json(book).to_string(),
    ).into_response()
}
