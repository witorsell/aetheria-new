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
use std::net::{SocketAddr, ToSocketAddrs};

#[derive(Deserialize)]
pub struct ProxyQuery {
    url: String,
}

const PROXY_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

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

struct ResolvedTarget {
    host: String,
    pinned_addr: Option<SocketAddr>,
}

/// validates a proxy target's scheme and host, and - for a DNS name - pins
/// the resolved address so a DNS-rebind between this check and the actual
/// connect can't swap in a private IP behind our back. shared by
/// proxy_fetch_with_checks and proxy_image so there's exactly one copy of
/// this logic to keep correct instead of two that can drift apart.
async fn resolve_target(parsed_url: &Url) -> Result<ResolvedTarget, StatusCode> {
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let Some(host_str) = parsed_url.host_str() else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let host = host_str.to_string();

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err(StatusCode::FORBIDDEN);
        }
        return Ok(ResolvedTarget { host, pinned_addr: None });
    }

    if host == "localhost" {
        return Err(StatusCode::FORBIDDEN);
    }

    let port = parsed_url.port_or_known_default().unwrap_or(80);
    let lookup_host = host.clone();
    // std's resolver is synchronous; run it on a blocking thread instead of
    // an async worker so a slow or hanging DNS lookup can't stall other
    // requests sharing the runtime
    let resolved = tokio::task::spawn_blocking(move || (lookup_host.as_str(), port).to_socket_addrs())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let mut first_addr = None;
    let mut any_resolved = false;
    for socket_addr in resolved {
        any_resolved = true;
        if is_private_ip(socket_addr.ip()) {
            return Err(StatusCode::FORBIDDEN);
        }
        first_addr.get_or_insert(socket_addr);
    }
    if !any_resolved {
        return Err(StatusCode::BAD_REQUEST);
    }

    Ok(ResolvedTarget { host, pinned_addr: first_addr })
}

pub async fn proxy_fetch_with_checks(url: &str) -> Result<reqwest::Response, StatusCode> {
    let parsed_url = Url::parse(url).map_err(|_| StatusCode::BAD_REQUEST)?;
    let target = resolve_target(&parsed_url).await?;

    let mut client_builder = Client::builder()
        .user_agent(PROXY_USER_AGENT)
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(addr) = target.pinned_addr {
        client_builder = client_builder.resolve(&target.host, addr);
    }
    let client = client_builder.build().unwrap_or_default();
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

    let target = match resolve_target(&parsed_url).await {
        Ok(target) => target,
        Err(StatusCode::FORBIDDEN) => {
            return (StatusCode::FORBIDDEN, "Access to private or local addresses is forbidden").into_response();
        }
        Err(StatusCode::INTERNAL_SERVER_ERROR) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to resolve host").into_response();
        }
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid or unresolvable URL").into_response(),
    };

    // check cache first
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

    let mut client_builder = Client::builder()
        .user_agent(PROXY_USER_AGENT)
        .redirect(reqwest::redirect::Policy::none());
    if let Some(addr) = target.pinned_addr {
        client_builder = client_builder.resolve(&target.host, addr);
    }
    let client = client_builder.build().unwrap_or_default();
    let res = match client.get(&query.url).send().await {
        Ok(res) => res,
        Err(e) => {
            tracing::warn!(url = %query.url, error = %e, "proxy fetch failed");
            return (StatusCode::BAD_GATEWAY, "Failed to fetch image").into_response();
        }
    };

    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();

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
}
