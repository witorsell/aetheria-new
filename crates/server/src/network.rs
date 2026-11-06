use crate::error::ApiError;
use reqwest::dns::Resolve;
use std::collections::HashSet;
use std::sync::Arc;
use url::{Host, Url};

#[derive(Clone, Default)]
pub struct ProviderNetworkPolicy {
    allowed_origins: HashSet<String>,
}

fn parse_endpoint(value: &str) -> Result<Url, ApiError> {
    let url = Url::parse(value).map_err(|_| ApiError::bad_request("Invalid provider URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none()
        || !url.username().is_empty() || url.password().is_some()
        || url.query().is_some() || url.fragment().is_some() {
        return Err(ApiError::bad_request("Provider URL must be an http(s) URL without credentials, query, or fragment"));
    }
    Ok(url)
}

impl ProviderNetworkPolicy {
    pub fn from_env() -> Self {
        let origins = std::env::var("AETHERIA_PROVIDER_ALLOWED_ORIGINS").unwrap_or_default();
        Self::new(origins.split(',').map(str::trim).filter(|s| !s.is_empty()))
            .expect("AETHERIA_PROVIDER_ALLOWED_ORIGINS must contain comma-separated http(s) origins")
    }

    pub fn new<'a>(origins: impl IntoIterator<Item = &'a str>) -> Result<Self, ApiError> {
        let mut allowed_origins = HashSet::new();
        for origin in origins {
            let url = parse_endpoint(origin)?;
            if url.path() != "/" {
                return Err(ApiError::bad_request("Allowed provider origins cannot contain a path"));
            }
            allowed_origins.insert(url.origin().ascii_serialization());
        }
        Ok(Self { allowed_origins })
    }

    pub fn client(&self, endpoint: &str) -> Result<reqwest::Client, ApiError> {
        let url = parse_endpoint(endpoint)?;
        let allow_private = self.allowed_origins.contains(&url.origin().ascii_serialization());
        let private_literal = match url.host() {
            Some(Host::Ipv4(ip)) => crate::routes::proxy::is_private_ip(ip.into()),
            Some(Host::Ipv6(ip)) => crate::routes::proxy::is_private_ip(ip.into()),
            Some(Host::Domain(host)) => host.trim_end_matches('.').eq_ignore_ascii_case("localhost"),
            None => true,
        };
        if private_literal && !allow_private {
            return Err(ApiError::forbidden("Private provider endpoints must be allowed by the instance administrator"));
        }
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .pool_max_idle_per_host(4)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .dns_resolver(ProviderResolver { allow_private })
            .build()
            .map_err(|_| ApiError::internal("Failed to build provider client"))
    }
}

struct ProviderResolver {
    allow_private: bool,
}

impl Resolve for ProviderResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        if !self.allow_private {
            return crate::routes::proxy::SafeResolver.resolve(name);
        }
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host.as_str(), 0)).await?;
            let addresses: Vec<_> = addresses.collect();
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

pub async fn provider_client(
    policy: &ProviderNetworkPolicy,
    cache: &moka::future::Cache<String, reqwest::Client>,
    endpoint: &str,
) -> Result<reqwest::Client, ApiError> {
    let origin = parse_endpoint(endpoint)?.origin().ascii_serialization();
    cache.try_get_with(origin, async { policy.client(endpoint) }).await
        .map_err(|error: Arc<ApiError>| ApiError::new(error.code, error.message.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_endpoints_require_an_exact_origin_allowlist() {
        let policy = ProviderNetworkPolicy::new(["http://127.0.0.1:5001"]).unwrap();
        assert!(policy.client("http://127.0.0.1:5001/v1").is_ok());
        for url in ["http://127.0.0.1:5002/v1", "http://[::1]:5001/v1", "http://169.254.169.254", "file:///etc/passwd", "https://user:pass@example.com", "http://localhost:5001"] {
            assert!(policy.client(url).is_err(), "{url}");
        }
        assert!(policy.client("https://example.com/v1").is_ok());
    }

    #[tokio::test]
    async fn allowed_providers_cannot_redirect_to_another_endpoint() {
        use axum::{routing::get, Router};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().route("/models", get(|| async {
            axum::response::Redirect::temporary("http://169.254.169.254/models")
        }));
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let policy = ProviderNetworkPolicy::new([origin.as_str()]).unwrap();
        let response = policy.client(&origin).unwrap().get(format!("{origin}/models")).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);
        task.abort();
    }
}
