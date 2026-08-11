use crate::db::Db;
use axum::extract::FromRef;
use cookie::Key;
use moka::future::Cache;
use std::time::Duration;

/// idle-timeout window: a session is considered expired after this many
/// minutes of inactivity. configurable via env var, defaults to 7 days.
fn idle_timeout() -> Duration {
    let secs = std::env::var("AETHERIA_SESSION_IDLE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(7 * 24 * 60 * 60);
    Duration::from_secs(secs)
}

/// absolute maximum session age: even if the session is used continuously,
/// it expires once this many seconds have elapsed since issuance.
/// configurable via env var, defaults to 30 days.
fn absolute_max_age() -> Duration {
    let secs = std::env::var("AETHERIA_SESSION_ABSOLUTE_MAX_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30 * 24 * 60 * 60);
    Duration::from_secs(secs)
}

#[derive(Clone)]
pub struct AppState {
    pub db: std::sync::Arc<Db>,
    pub cookie_key: Key,
    pub encryption_key: [u8; 32],
    pub image_cache: Cache<String, CachedImage>,
    /// shared, pooled HTTP client for all provider and embedding API calls.
    /// reused across requests so connection-pool / DNS-cache / TLS handshake
    /// costs are paid once, not per-generation.
    pub http_client: reqwest::Client,
    /// failed login attempts per username. keyed lowercase so the
    /// lockout can't be dodged by varying case. TTL doubles as the lockout
    /// window: every failed attempt refreshes it, so a username under active
    /// brute-force stays locked as long as attempts keep coming in, and
    /// clears itself 15 minutes after they stop.
    pub login_attempts: Cache<String, u32>,
    pub registration_enabled: bool,
    /// idle timeout and absolute max age for sessions
    pub session_idle_timeout: Duration,
    pub session_absolute_max_age: Duration,
}

#[derive(Clone)]
pub struct CachedImage {
    pub content_type: String,
    pub bytes: bytes::Bytes,
}

impl AppState {
    pub fn new(db: Db, registration_enabled: bool) -> Self {
        let secret = std::env::var("AETHERIA_SESSION_SECRET")
            .expect("AETHERIA_SESSION_SECRET must be set (>= 64 random bytes, base64 or raw)");
        let encryption_key_str = std::env::var("AETHERIA_ENCRYPTION_KEY")
            .expect("AETHERIA_ENCRYPTION_KEY must be set (exactly 32 bytes)");

        let session_idle_timeout = idle_timeout();
        let session_absolute_max_age = absolute_max_age();

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .pool_max_idle_per_host(4)
            .build()
            .expect("building reqwest client should not fail");

        Self {
            db: std::sync::Arc::new(db),
            cookie_key: Key::derive_from(secret.as_bytes()),
            encryption_key: encryption_key_str
                .as_bytes()
                .try_into()
                .expect("AETHERIA_ENCRYPTION_KEY must be exactly 32 bytes"),
            image_cache: Cache::builder()
                .max_capacity(500)
                .time_to_live(Duration::from_secs(3600))
                .build(),
            http_client,
            login_attempts: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(15 * 60))
                .build(),
            registration_enabled,
            session_idle_timeout,
            session_absolute_max_age,
        }
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}
