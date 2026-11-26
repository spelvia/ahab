//! The Agent Client Protocol's registry of agents
//! (<https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json>):
//! fetched now and then, read with Stubb.

use std::time::Duration;

/// Fetches the registry's current index.
pub async fn fetch() -> Result<stubb::providers::acp::registry::Registry, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(stubb::providers::acp::registry::INDEX_URL)
        .send()
        .await
        .map_err(|error| format!("the ACP registry cannot be reached: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("the ACP registry answered {}", response.status()));
    }
    let text = response
        .text()
        .await
        .map_err(|error| format!("the ACP registry's answer is unreadable: {error}"))?;
    stubb::providers::acp::registry::Registry::parse(&text)
        .map_err(|error| format!("the ACP registry's answer is unreadable: {error}"))
}

/// Fetches a registry agent's logo (an SVG, at most Stubb's limit).
pub async fn fetch_logo(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("the logo cannot be reached: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("the logo's server answered {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("the logo is unreadable: {error}"))?;
    if bytes.len() > stubb::discovery::profile::MAX_LOGO {
        return Err("the logo is too large".to_owned());
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| "the logo is not text".to_owned())
}
