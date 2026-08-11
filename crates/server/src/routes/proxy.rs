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

fn is_private_ip(ip: IpAddr) -> bool {
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

pub async fn proxy_image(Extension(_user_id): Extension<i64>, 
    State(state): State<AppState>,
    Query(query): Query<ProxyQuery>,
) -> impl IntoResponse {
    // validate URL scheme
    let parsed_url = match Url::parse(&query.url) {
        Ok(url) => url,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid URL").into_response(),
    };
    
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return (StatusCode::BAD_REQUEST, "Only HTTP/HTTPS allowed").into_response();
    }
    
    let Some(host_str) = parsed_url.host_str() else {
        return (StatusCode::BAD_REQUEST, "Invalid host").into_response();
    };

    // gotta pin the addr if host is a dns name not a literal ip, or reqwest
    // re-resolves on its own at connect time and a dns rebind snuck in
    // between our check and the connect just walks right past this whole
    // thing and hits a private ip anyway
    let mut pinned_addr: Option<SocketAddr> = None;

    if let Ok(ip) = host_str.parse::<IpAddr>() {
        if is_private_ip(ip) {
            return (StatusCode::FORBIDDEN, "Access to private IP is forbidden").into_response();
        }
    } else {
        if host_str == "localhost" {
            return (StatusCode::FORBIDDEN, "Access to localhost is forbidden").into_response();
        }

        let port = parsed_url.port_or_known_default().unwrap_or(80);
        let Ok(resolved) = (host_str, port).to_socket_addrs() else {
            return (StatusCode::BAD_REQUEST, "Could not resolve host").into_response();
        };

        let mut first_addr = None;
        let mut any_resolved = false;
        for socket_addr in resolved {
            any_resolved = true;
            if is_private_ip(socket_addr.ip()) {
                return (StatusCode::FORBIDDEN, "Resolved IP is private").into_response();
            }
            first_addr.get_or_insert(socket_addr);
        }
        if !any_resolved {
            return (StatusCode::BAD_REQUEST, "Could not resolve host").into_response();
        }
        pinned_addr = first_addr;
    }

    // check cache first
    if let Some(cached) = state.image_cache.get(&query.url).await {


        return (
            [
                (axum::http::header::CONTENT_TYPE, HeaderValue::from_str(&cached.content_type).unwrap()),
                (axum::http::header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600, immutable")),
            ],
            cached.bytes,
        )
            .into_response();
    }

    let mut client_builder = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
        .redirect(reqwest::redirect::Policy::none());
    if let Some(addr) = pinned_addr {
        client_builder = client_builder.resolve(host_str, addr);
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
            (axum::http::header::CONTENT_TYPE, HeaderValue::from_str(&content_type).unwrap()),
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
}
