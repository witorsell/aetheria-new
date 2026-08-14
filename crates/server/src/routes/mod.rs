use crate::state::AppState;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::catch_panic::CatchPanicLayer;
use tower::ServiceBuilder;

fn base_path() -> std::path::PathBuf {
    crate::resolve_path(".")
}

fn get_max_upload_bytes() -> usize {
    std::env::var("MAX_UPLOAD_SIZE_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|mb| mb * 1024 * 1024)
        .unwrap_or(25 * 1024 * 1024)
}

pub mod account;
pub mod chats;
pub mod characters;
pub mod generate;
pub mod generation_orchestrator;
pub mod settings;
pub mod import_export;
pub mod proxy;
pub mod lorebooks;
pub mod personas;
pub mod presets;
pub mod themes;
pub mod regex_scripts;
pub mod groups;

pub fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        // characters
        
        .route("/api/characters", get(characters::list).post(characters::create))
        .route("/api/characters/{id}", get(characters::get_character).put(characters::update).delete(characters::delete))
        .route("/api/characters/{id}/generate", post(generate::generate_character_field))
        .route("/api/characters/{id}/chats", get(chats::list_for_character).post(chats::create))
        // alternate greetings
        .route("/api/characters/{id}/greetings", get(characters::list_greetings).post(characters::add_greeting))
        .route("/api/characters/{id}/greetings/{gid}", put(characters::update_greeting).delete(characters::delete_greeting))
        // character tags
        .route("/api/characters/{id}/tags", get(characters::list_character_tags).put(characters::set_character_tags))
        // tags
        .route("/api/character-tags", get(characters::list_all_character_tags))
        .route("/api/tags", get(characters::list_tags).post(characters::create_tag))
        .route("/api/tags/{id}", delete(characters::delete_tag))
        // folders
        .route("/api/folders", get(characters::list_folders).post(characters::create_folder))
        .route("/api/folders/{id}", put(characters::update_folder).delete(characters::delete_folder))
        // avatar upload
        .route("/api/characters/{id}/avatar", post(characters::upload_avatar))
        // personas
        .route("/api/personas", get(personas::list).post(personas::create))
        .route("/api/personas/{id}", axum::routing::patch(personas::update).delete(personas::delete))
        .route("/api/personas/{id}/avatar", post(personas::upload_avatar))
        .route("/api/personas/active", post(personas::set_active))
        // lorebooks
        .route("/api/lorebooks", get(lorebooks::list).post(lorebooks::create))
        .route("/api/lorebooks/{id}", get(lorebooks::get_lorebook).put(lorebooks::update).delete(lorebooks::delete_lorebook))
        .route("/api/lorebooks/{id}/entries", get(lorebooks::list_entries).post(lorebooks::create_entry))
        .route("/api/lorebooks/{lid}/entries/{eid}", get(lorebooks::get_entry).put(lorebooks::update_entry).delete(lorebooks::delete_entry))
        .route("/api/characters/{id}/lorebooks", get(lorebooks::get_character_lorebooks).put(lorebooks::set_character_lorebooks))
        .route("/api/chats/{id}/lorebooks", get(lorebooks::get_chat_lorebooks).put(lorebooks::set_chat_lorebooks))
        // presets
        .route("/api/presets", get(presets::list).post(presets::import))
        .route("/api/presets/{id}", get(presets::get_preset).delete(presets::delete))
        .route("/api/presets/{id}/export", get(presets::export_preset))
        .route("/api/presets/{id}/order", put(presets::update_order))
        .route("/api/presets/activate", post(presets::activate))
        // themes
        .route("/api/themes", get(themes::list).post(themes::create))
        .route("/api/themes/active", get(themes::get_active))
        .route("/api/themes/{id}", get(themes::get_theme).put(themes::update).delete(themes::delete))
        .route("/api/themes/{id}/export", get(themes::export_theme))
        .route("/api/themes/activate", post(themes::activate))
        .route("/api/themes/import", post(themes::import))
        .route("/api/themes/import-st", post(themes::import_st))
        // regex scripts
        .route("/api/regex-scripts", get(regex_scripts::list).post(regex_scripts::import))
        .route("/api/regex-scripts/export", get(regex_scripts::export_all))
        .route("/api/regex-scripts/{id}", delete(regex_scripts::delete))
        .route("/api/regex-scripts/{id}/disabled", put(regex_scripts::set_disabled))
        // groups
        .route("/api/groups", get(groups::list).post(groups::create))
        .route("/api/groups/{id}", get(groups::get_group).put(groups::update).delete(groups::delete_group))
        .route("/api/groups/{id}/members", put(groups::set_members))
        .route("/api/groups/{id}/chats", post(groups::create_chat))
        // import/export
        .route("/api/import/character", post(import_export::import_character))
        .route("/api/export/character/{id}", get(import_export::export_character))
        .route("/api/import/lorebook", post(import_export::import_lorebook))
        .route("/api/export/lorebook/{id}", get(import_export::export_lorebook))
        // chat / messages / generation
        .route("/api/chats/{id}", get(chats::get_chat))
        .route("/api/chats/{id}/messages", get(chats::get_tree))
        .route("/api/chats/{id}/active_branch", get(chats::get_active_branch))
        .route("/api/chats/{id}/members", post(chats::add_member))
        .route("/api/chats/{id}/members/{character_id}", delete(chats::remove_member))
        .route("/api/chats/{id}/generate", post(generate::generate))
        .route("/api/chats/{id}/regenerate", post(generate::regenerate))
        .route("/api/chats/{id}/continue", post(generate::continue_generation))
        .route("/api/chats/{id}/respond-as-user", post(generate::respond_as_user))
        .route(
            "/api/messages/{id}",
            delete(chats::delete_message).patch(chats::edit_message),
        )
        .route("/api/messages/{id}/visible", put(chats::set_message_visibility))
        .route("/api/settings", get(settings::get).put(settings::update))
        .route("/api/chats/{id}/messages/tree", get(chats::tree_from_message))
        .route("/api/settings/export", get(settings::export))
        .route("/api/settings/import", post(settings::import))
        .route("/api/settings/models", get(settings::list_models))
        .route("/api/account/export-all", get(account::export_all))
        .route("/api/account/data", delete(account::delete_all))
        .route("/api/account/import-all", post(account::import_all))
        .route("/api/proxy", get(proxy::proxy_image))
        .route("/api/logout", post(crate::auth::logout))
        .route("/api/me", get(crate::auth::me).put(crate::auth::update_me))
        .route("/api/me/avatar", post(crate::auth::upload_my_avatar))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::require_auth,
        ))
        .with_state(state.clone());

    let protected_v1 = protected.clone();

    // avatar filenames are predictable (avatar_user_{id}.{ext},
    // avatar_{character_id}.{ext}), so this can't just sit outside the
    // protected router the way a plain static-file mount normally would -
    // anyone who knows or guesses an id could view someone else's avatar
    // without ever logging in. gated by the same session-cookie middleware
    // as everything else, which a same-origin <img src> still sends fine.
    let uploads = Router::new()
        .nest_service("/uploads", ServeDir::new(base_path().join("crates/server/uploads")))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::require_auth,
        ))
        .with_state(state.clone());

    let public_routes = Router::new()
        .route("/health", get(health))
        .route("/api/health", get(health))
        .route("/api/v1/health", get(health))
        .route("/api/login", post(crate::auth::login))
        .route("/api/v1/login", post(crate::auth::login))
        .route("/api/register", post(crate::auth::register))
        .route("/api/v1/register", post(crate::auth::register))
        .route("/api/registration-status", get(crate::auth::registration_status))
        .route("/api/v1/registration-status", get(crate::auth::registration_status))
        .with_state(state);

    Router::new()
        .merge(public_routes)
        .merge(uploads)
        .merge(protected)
        .nest("/api/v1", protected_v1)
        .layer(DefaultBodyLimit::max(get_max_upload_bytes()))
        .layer(CatchPanicLayer::new())
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https:; media-src 'self' data: blob: https:; connect-src 'self' ws: wss:; font-src 'self' data:;",
            ),
        ))
        .fallback_service(
            ServeDir::new(base_path().join("crates/frontend/dist")).fallback(
                ServiceBuilder::new()
                    .layer(SetResponseHeaderLayer::overriding(
                        header::CACHE_CONTROL,
                        HeaderValue::from_static("no-cache"),
                    ))
                    .service(ServeFile::new(base_path().join("crates/frontend/dist/index.html"))),
            ),
        )
}

async fn health(State(state): State<AppState>) -> StatusCode {
    if sqlx::query("SELECT 1").execute(&state.db.read_pool).await.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
