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

#[derive(Debug)]
pub struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate_per_sec: f64,
    last_update: std::time::Instant,
}

impl TokenBucket {
    pub fn new(max_tokens: f64, refill_rate_per_sec: f64) -> Self {
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_rate_per_sec,
            last_update: std::time::Instant::now(),
        }
    }

    pub fn try_consume(&mut self, tokens: f64) -> bool {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate_per_sec).min(self.max_tokens);
        self.last_update = now;

        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }
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
    /// shared, pooled HTTP client for proxying arbitrary user-supplied image/
    /// avatar URLs. kept separate from `http_client` since it carries SSRF
    /// protections (see `routes::proxy::SafeResolver`) and proxy-specific
    /// settings (spoofed user agent, no redirects) that provider calls to
    /// known/trusted endpoints don't need.
    pub proxy_client: reqwest::Client,
    /// failed login attempts per username. keyed lowercase so the
    /// lockout can't be dodged by varying case. TTL doubles as the lockout
    /// window: every failed attempt refreshes it, so a username under active
    /// brute-force stays locked as long as attempts keep coming in, and
    /// clears itself 15 minutes after they stop.
    pub login_attempts: Cache<String, u32>,
    /// failed login attempts per client IP, same TTL/window semantics as
    /// login_attempts above. the per-username lockout alone doesn't stop an
    /// attacker spraying many different usernames from one IP - each one
    /// individually stays under that username's own threshold forever.
    /// same 15-minute self-clearing window, but a much higher threshold:
    /// this exists to catch spraying, not to duplicate the per-account
    /// lockout, and a shared/NAT'd IP can represent several real users.
    pub login_attempts_by_ip: Cache<String, u32>,
    /// rate limiter token buckets for text generation endpoints per user
    pub generation_rate_limiter: Cache<i64, std::sync::Arc<tokio::sync::Mutex<TokenBucket>>>,
    /// serializes maybe_update_chat_summary passes per chat_id. it's spawned
    /// fire-and-forget after every reply, and without this two passes landing
    /// close together (fast group chat members, a quick regenerate) would
    /// both read the same cursor/summary before either writes - whichever
    /// LLM call finishes last wins and silently discards or regresses the
    /// other pass's work
    pub summary_locks: Cache<String, std::sync::Arc<tokio::sync::Mutex<()>>>,
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

        let img_cache_cap = std::env::var("AETHERIA_IMAGE_CACHE_MAX_CAPACITY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500);
        let img_cache_ttl = std::env::var("AETHERIA_IMAGE_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3600);

        Self {
            db: std::sync::Arc::new(db),
            cookie_key: Key::derive_from(secret.as_bytes()),
            encryption_key: encryption_key_str
                .as_bytes()
                .try_into()
                .expect("AETHERIA_ENCRYPTION_KEY must be exactly 32 bytes"),
            image_cache: Cache::builder()
                .max_capacity(img_cache_cap)
                .time_to_live(Duration::from_secs(img_cache_ttl))
                .build(),
            http_client,
            proxy_client: crate::routes::proxy::build_proxy_client(),
            login_attempts: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(15 * 60))
                .build(),
            login_attempts_by_ip: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(15 * 60))
                .build(),
            generation_rate_limiter: Cache::builder()
                .max_capacity(10_000)
                .time_to_idle(Duration::from_secs(3600))
                .build(),
            summary_locks: Cache::builder()
                .max_capacity(10_000)
                .time_to_idle(Duration::from_secs(3600))
                .build(),
            registration_enabled,
            session_idle_timeout,
            session_absolute_max_age,
        }
    }

    pub async fn check_generation_rate_limit(&self, user_id: i64) -> Result<(), crate::error::ApiError> {
        let bucket_arc = match self.generation_rate_limiter.get(&user_id).await {
            Some(b) => b,
            None => {
                let max_tokens = std::env::var("AETHERIA_GENERATE_BURST")
                    .ok()
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(20.0);
                let refill_per_sec = std::env::var("AETHERIA_GENERATE_PER_SEC")
                    .ok()
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(0.5);
                let b = std::sync::Arc::new(tokio::sync::Mutex::new(TokenBucket::new(max_tokens, refill_per_sec)));
                self.generation_rate_limiter.insert(user_id, b.clone()).await;
                b
            }
        };
        let mut bucket = bucket_arc.lock().await;
        if bucket.try_consume(1.0) {
            Ok(())
        } else {
            Err(crate::error::ApiError::too_many_requests("Rate limit exceeded for generation endpoint"))
        }
    }

    /// the lock guarding a chat's memory-summary read-summarize-write
    /// sequence, created on first use. uses moka's get_with (single-flight)
    /// rather than a plain get-then-insert: two calls racing on the same
    /// missing chat_id under a plain get/insert could each create their own
    /// Arc<Mutex<()>>, and locking two different mutexes wouldn't serialize
    /// anything - the exact race this lock exists to close.
    pub async fn chat_summary_lock(&self, chat_id: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
        self.summary_locks
            .get_with(chat_id.to_string(), async { std::sync::Arc::new(tokio::sync::Mutex::new(())) })
            .await
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_bucket_consumes_and_limits() {
        let mut bucket = TokenBucket::new(2.0, 0.0);
        assert!(bucket.try_consume(1.0));
        assert!(bucket.try_consume(1.0));
        assert!(!bucket.try_consume(1.0));
    }
}
