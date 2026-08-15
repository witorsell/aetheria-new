use axum::extract::Extension;
use crate::state::{AppState, CachedImage};
use axum::{
    extract::{Query, State},
    http::{HeaderValue, StatusCode},
    response::IntoResponse,
};
use reqwest::Client;
use serde::Deserialize;
use url::Url;
use std::net::IpAddr;
use ipnet::IpNet;
use std::net::ToSocketAddrs;

#[derive(Deserialize)]
pub struct ProxyQuery {
    url: String,
}

pub const PROXY_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

// avatars/banners can be a static image, an animated gif, or a short video
// clip - gif already falls under the `image/` content-type prefix, so only
// video needs its own prefix check below.
const ALLOWED_MEDIA_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "avif", "bmp", "svg",
    "mp4", "webm", "mov", "m4v",
];

fn is_media_content_type(content_type: &str) -> bool {
    let ct = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    ct.starts_with("image/") || ct.starts_with("video/")
}

fn url_has_media_extension(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|u| u.path_segments().and_then(|mut s| s.next_back().map(str::to_string)))
        .and_then(|last| last.rsplit('.').next().map(str::to_ascii_lowercase))
        .map(|ext| ALLOWED_MEDIA_EXTENSIONS.contains(&ext.as_str()))
        .unwrap_or(false)
}

fn is_private_ip(ip: IpAddr) -> bool {
    // normalize an IPv4-mapped IPv6 literal (::ffff:a.b.c.d) to its IPv4
    // form first - the CIDR list below is IPv4/IPv6 native only, so a
    // mapped literal like ::ffff:127.0.0.1 would silently miss every entry
    // in it despite the OS treating a connection to it as going to 127.0.0.1
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(v6)),
        other => other,
    };

    let private_networks = [
        "127.0.0.0/8",      // loopback
        "10.0.0.0/8",       // private lan
        "172.16.0.0/12",    // private lan
        "192.168.0.0/16",   // private lan
        "169.254.0.0/16",   // link-local
        "::1/128",          // ipv6 loopback
        "fc00::/7",         // unique local
        "fe80::/10",        // link-local
    ];

    for net in private_networks.iter() {
        if let Ok(ipnet) = net.parse::<IpNet>() {
            if ipnet.contains(&ip) {
                return true;
            }
        }
    }
    false
}

/// fast, no-network check on a proxy target before any request is attempted:
/// rejects bad schemes and a literal private/loopback IP or `localhost` host
/// immediately. a DNS-name host is deliberately *not* resolved here - hyper
/// only calls out to the resolver at actual connect time, so pre-resolving
/// here would just be a check that a later DNS answer could disagree with.
/// `SafeResolver` below does the real enforcement, for every connection the
/// shared client ever makes, not just an initial check.
fn validate_target(parsed_url: &Url) -> Result<(), StatusCode> {
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return Err(StatusCode::BAD_REQUEST);
    }
    let Some(host_str) = parsed_url.host_str() else {
        return Err(StatusCode::BAD_REQUEST);
    };
    if let Ok(ip) = host_str.parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err(StatusCode::FORBIDDEN);
        }
    } else if host_str.eq_ignore_ascii_case("localhost") {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

/// custom DNS resolver installed on the shared proxy client (`build_proxy_client`
/// below) that refuses to resolve a hostname to any private/loopback/link-local
/// address. this is what makes it safe to reuse one pooled `Client` - with real
/// connection/TLS-session reuse - across arbitrary user-supplied proxy targets:
/// a plain shared client would resolve DNS itself, at true connect time, with no
/// chance for us to inspect the answer first. previously this route rebuilt a
/// whole fresh `Client` per request instead, resolving up front and pinning that
/// one address into the client's config, purely to get a look at the answer
/// before connecting - at the cost of a fresh TCP+TLS handshake every time, even
/// to a host it had just talked to. installing this resolver means the pooled
/// client's own real resolution is the checked one, every time it happens
/// (including on every new connection a long-lived pool eventually needs to
/// open, not just the first), so no separate pin or per-request client is
/// needed to stay safe against DNS-rebind between check and connect.
pub struct SafeResolver;

impl reqwest::dns::Resolve for SafeResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            if host.eq_ignore_ascii_case("localhost") {
                return Err(format!("resolution of {host} is not allowed").into());
            }

            let lookup_host = host.clone();
            // std's resolver is synchronous; run it on a blocking thread instead of
            // an async worker so a slow or hanging DNS lookup can't stall other
            // requests sharing the runtime
            let resolved = tokio::task::spawn_blocking(move || (lookup_host.as_str(), 0u16).to_socket_addrs())
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

            let addrs: Vec<std::net::SocketAddr> = resolved.collect();
            if addrs.is_empty() {
                return Err(format!("no addresses resolved for {host}").into());
            }
            for addr in &addrs {
                if is_private_ip(addr.ip()) {
                    return Err(format!("{host} resolves to a private/internal address").into());
                }
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// builds the client `AppState::proxy_client` holds and every proxy request
/// reuses - built once at startup, not per request, so repeat fetches to the
/// same host (a CDN serving lots of avatars, for instance) get real
/// connection/TLS-session pooling instead of a fresh handshake every time.
/// stays SSRF-safe despite being shared and long-lived via `SafeResolver`.
pub fn build_proxy_client() -> Client {
    Client::builder()
        .user_agent(PROXY_USER_AGENT)
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(SafeResolver)
        .build()
        .expect("building proxy reqwest client should not fail")
}

pub async fn proxy_fetch_with_checks(client: &Client, url: &str) -> Result<reqwest::Response, StatusCode> {
    let parsed_url = Url::parse(url).map_err(|_| StatusCode::BAD_REQUEST)?;
    validate_target(&parsed_url)?;
    client
        .get(url)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(url, error = %e, "proxy fetch failed");
            StatusCode::BAD_GATEWAY
        })
}

pub async fn proxy_image(Extension(_user_id): Extension<i64>,
    State(state): State<AppState>,
    Query(query): Query<ProxyQuery>,
) -> impl IntoResponse {
    let parsed_url = match Url::parse(&query.url) {
        Ok(url) => url,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid URL").into_response(),
    };

    if let Err(status) = validate_target(&parsed_url) {
        return match status {
            StatusCode::FORBIDDEN => (StatusCode::FORBIDDEN, "Access to private or local addresses is forbidden").into_response(),
            _ => (StatusCode::BAD_REQUEST, "Invalid URL").into_response(),
        };
    }

    // cache check before ever touching the network: a cache hit should be a
    // pure in-memory lookup, not pay for a DNS round-trip or connection it
    // doesn't need.
    if let Some(cached) = state.image_cache.get(&query.url).await {
        return (
            [
                (axum::http::header::CONTENT_TYPE, HeaderValue::from_str(&cached.content_type).unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"))),
                (axum::http::header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600, immutable")),
            ],
            cached.bytes,
        )
            .into_response();
    }

    let res = match state.proxy_client.get(&query.url).send().await {
        Ok(res) => res,
        Err(e) => {
            tracing::warn!(url = %query.url, error = %e, "proxy fetch failed");
            return (StatusCode::BAD_GATEWAY, "Failed to fetch image").into_response();
        }
    };

    let content_type_header = res
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);

    // this is an image/video proxy, not a general-purpose fetch - refuse
    // anything that isn't reporting (or, absent a header, isn't named like)
    // media, so an authenticated user can't turn this into a way to read back
    // arbitrary JSON/HTML/files from any non-private host.
    let is_media = content_type_header.as_deref().map(is_media_content_type).unwrap_or(false)
        || (content_type_header.is_none() && url_has_media_extension(&query.url));
    if !is_media {
        tracing::warn!(url = %query.url, content_type = ?content_type_header, "proxy target is not image/video media, refusing");
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "URL did not return an image or video").into_response();
    }
    let content_type = content_type_header.unwrap_or_else(|| "image/jpeg".to_string());

    let bytes = match res.bytes().await {
        Ok(bytes) => bytes,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read image body").into_response(),
    };

    // store in cache
    state
        .image_cache
        .insert(query.url.clone(), CachedImage {
            content_type: content_type.clone(),
            bytes: bytes.clone(),
        })
        .await;

    (
        [
            (axum::http::header::CONTENT_TYPE, HeaderValue::from_str(&content_type).unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"))),
            (axum::http::header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600, immutable")),
        ],
        bytes,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn private_ips_are_blocked() {
        let private_ips = [
            "127.0.0.1",
            "10.0.0.5",
            "172.16.0.1",
            "192.168.1.100",
            "169.254.1.1",
            "::1",
            "fc00::1",
            "fe80::1",
        ];
        for ip_str in private_ips {
            let ip: IpAddr = ip_str.parse().expect("valid IP");
            assert!(is_private_ip(ip), "IP {ip_str} must be flagged as private");
        }
    }

    #[test]
    fn public_ips_are_allowed() {
        let public_ips = [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.216.34",
            "2606:4700:4700::1111",
        ];
        for ip_str in public_ips {
            let ip: IpAddr = ip_str.parse().expect("valid IP");
            assert!(!is_private_ip(ip), "IP {ip_str} must not be flagged as private");
        }
    }

    #[test]
    fn ipv4_mapped_ipv6_private_addresses_are_blocked() {
        // the OS treats a connection to ::ffff:a.b.c.d as a connection to
        // a.b.c.d, so these have to be caught the same as their plain
        // IPv4 form or they're a straight SSRF bypass
        let mapped_private_ips = [
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.5",
            "::ffff:172.16.0.1",
            "::ffff:192.168.1.100",
            "::ffff:169.254.1.1",
        ];
        for ip_str in mapped_private_ips {
            let ip: IpAddr = ip_str.parse().expect("valid IP");
            assert!(is_private_ip(ip), "mapped IP {ip_str} must be flagged as private");
        }
    }

    #[test]
    fn ipv4_mapped_ipv6_public_addresses_are_allowed() {
        let ip: IpAddr = "::ffff:8.8.8.8".parse().expect("valid IP");
        assert!(!is_private_ip(ip), "mapped public IP must not be flagged as private");
    }

    #[test]
    fn image_and_video_content_types_are_allowed() {
        for ct in ["image/png", "image/webp", "image/gif", "video/mp4", "video/webm", "IMAGE/JPEG", "image/png; charset=utf-8"] {
            assert!(is_media_content_type(ct), "{ct} should be treated as media");
        }
    }

    #[test]
    fn non_media_content_types_are_rejected() {
        for ct in ["application/json", "text/html", "text/plain", "application/octet-stream", ""] {
            assert!(!is_media_content_type(ct), "{ct} should not be treated as media");
        }
    }

    #[test]
    fn media_file_extensions_are_recognized() {
        for url in [
            "https://cdn.example.com/foo.png",
            "https://cdn.example.com/foo.WEBP",
            "https://cdn.example.com/dir/foo.gif",
            "https://cdn.example.com/clip.mp4",
            "https://cdn.example.com/clip.webm",
        ] {
            assert!(url_has_media_extension(url), "{url} should be recognized as media by extension");
        }
    }

    #[test]
    fn non_media_file_extensions_are_not_recognized() {
        for url in [
            "https://api.example.com/data.json",
            "https://example.com/page.html",
            "https://example.com/no-extension",
        ] {
            assert!(!url_has_media_extension(url), "{url} should not be recognized as media by extension");
        }
    }
}
