//! Calling the server's API. A failed call prints its error and gives the
//! exit code. Every call carries this command line's credential for the
//! server (see `credentials.rs`).

use std::time::Duration;

use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::cli::credentials::{self, Login, Source};
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::config;

/// How long one call may take.
const TIMEOUT: Duration = Duration::from_secs(60);

/// The server's API.
#[derive(Debug, Clone)]
pub struct Client {
    base: String,
    json: bool,
    http: reqwest::Client,
    credential: Option<String>,
    source: Option<Source>,
}

impl Client {
    /// A client without a credential: for the public routes (health,
    /// pairing, joining).
    pub fn new(base: &str, json: bool) -> Result<Self, Exit> {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|error| {
                output::failure(json, Exit::Other, "Cannot start", &error.to_string())
            })?;
        Ok(Self {
            base: base.trim_end_matches('/').to_owned(),
            json,
            http,
            credential: None,
            source: None,
        })
    }

    /// A client with this command line's credential for `base`: fails with
    /// exit code 2, saying how to log in, when it has none.
    pub fn authenticated(base: &str, json: bool) -> Result<Self, Exit> {
        let client = Self::new(base, json)?;
        match credentials::find(&client.base, &config::process_env) {
            Some((credential, source)) => Ok(Self {
                credential: Some(credential),
                source: Some(source),
                ..client
            }),
            None => Err(credentials::missing(
                &client.base,
                json,
                &config::process_env,
            )),
        }
    }

    /// This client, sending `credential`.
    pub fn with_credential(self, credential: String) -> Self {
        Self {
            credential: Some(credential),
            source: Some(Source::Login),
            ..self
        }
    }

    /// The server's address, such as `https://ahab.example.internal`.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Whether to print the API's JSON instead of text.
    pub fn json(&self) -> bool {
        self.json
    }

    /// Whether the credential is this computer's own (the server runs
    /// here).
    pub fn is_local(&self) -> bool {
        matches!(self.source, Some(Source::Local(_)))
    }

    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let request = self.http.request(method, format!("{}{path}", self.base));
        match &self.credential {
            Some(credential) => request.bearer_auth(credential),
            None => request,
        }
    }

    pub async fn get(&self, path: &str) -> Result<Value, Exit> {
        self.send(Method::GET, path, None).await
    }

    /// A GET that prints nothing when it fails: `None` instead.
    pub async fn quiet_get(&self, path: &str) -> Option<Value> {
        let response = self.request(Method::GET, path).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json().await.ok()
    }

    /// A request with an empty JSON body that prints nothing: the answer,
    /// or the server's reason for refusing it.
    pub async fn try_send(&self, method: Method, path: &str) -> Result<Value, String> {
        let response = self
            .request(method, path)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let ok = response.status().is_success();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if ok {
            Ok(body)
        } else {
            Err(body["detail"]
                .as_str()
                .or_else(|| body["title"].as_str())
                .unwrap_or("refused")
                .to_owned())
        }
    }

    /// An answer read as the API type `T`. An answer of another shape (a
    /// server of another version) is an error.
    pub fn parse<T: DeserializeOwned>(&self, value: Value) -> Result<T, Exit> {
        serde_json::from_value(value).map_err(|error| {
            output::failure(
                self.json,
                Exit::Other,
                "Unexpected answer",
                &format!("The server's answer does not match this version of ahab: {error}"),
            )
        })
    }

    /// One call; `null` for an answer without a body.
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, Exit> {
        let mut request = self.request(method, path);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => return Err(self.unreachable(&error)),
        };
        self.read(response).await
    }

    /// A link that opens `path` (such as `/p/ahab`) in a browser, paired:
    /// the API makes a one-time pairing link with this command line's
    /// credential. A browser that is paired already just opens `path`.
    /// The plain address when no link can be made.
    pub async fn browser_link(&self, path: &str) -> String {
        let plain = format!("{}{path}", self.base);
        if self.credential.is_none() {
            return plain;
        }
        let Ok(created) = self.try_send(Method::POST, "/api/v1/pairings").await else {
            return plain;
        };
        match created["secret"].as_str() {
            Some(secret) if path.is_empty() || path == "/" => {
                format!("{}/pair#{secret}", self.base)
            }
            Some(secret) => format!("{}/pair?next={}#{secret}", self.base, urlencode(path)),
            None => plain,
        }
    }

    fn unreachable(&self, error: &reqwest::Error) -> Exit {
        output::failure(
            self.json,
            Exit::Temporary,
            "The server cannot be reached",
            &format!(
                "{}: {error}. Is it running? Start it with `ahab up`, or say where it \
                 is with --server.",
                self.base
            ),
        )
    }

    async fn read(&self, response: reqwest::Response) -> Result<Value, Exit> {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if status.is_success() {
            return Ok(if text.trim().is_empty() {
                Value::Null
            } else {
                serde_json::from_str(&text).unwrap_or(Value::String(text))
            });
        }
        let mut problem = serde_json::from_str::<Value>(&text)
            .ok()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({ "title": status.to_string(), "detail": text }));
        if problem["code"] == "not_paired" {
            // Logging in is the way out, as when there is no credential.
            problem["detail"] = json!(credentials::refused_hint(&self.base, self.source.as_ref()));
            output::error(self.json, &problem);
            return Err(Exit::Usage);
        }
        output::error(self.json, &problem);
        Err(Exit::from_status(status.as_u16()))
    }
}

/// `https://` when no scheme is given; no trailing slash.
pub fn normalize_server(input: &str) -> Option<String> {
    let input = input.trim();
    let with_scheme = if input.contains("://") {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    let url = Url::parse(&with_scheme).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    Some(with_scheme.trim_end_matches('/').to_owned())
}

/// Keeps this command line's credential for `server` (`ahab login`), in
/// the data folder, readable only by its owner.
pub fn remember_login(
    server: &str,
    credential: &str,
    client_id: Option<String>,
    name: Option<String>,
) -> Result<(), String> {
    let paths = config::Paths::resolve(&config::process_env).map_err(|error| error.to_string())?;
    credentials::save(
        &paths,
        server,
        Login {
            credential: credential.to_owned(),
            client_id,
            name,
        },
    )
}

/// Forgets this command line's login for `server` (`ahab logout`): its
/// credential and client ID, when there was one.
pub fn forget_login(server: &str) -> Result<Option<(String, Option<String>)>, String> {
    let paths = config::Paths::resolve(&config::process_env).map_err(|error| error.to_string())?;
    Ok(credentials::forget(&paths, server)?.map(|login| (login.credential, login.client_id)))
}

/// Percent-encodes a path segment.
pub fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_the_server_address() {
        assert_eq!(
            normalize_server("ahab.example.internal").as_deref(),
            Some("https://ahab.example.internal")
        );
        assert_eq!(
            normalize_server(" https://ahab.example.internal/ ").as_deref(),
            Some("https://ahab.example.internal")
        );
        assert_eq!(
            normalize_server("http://127.0.0.1:8180").as_deref(),
            Some("http://127.0.0.1:8180")
        );
        assert_eq!(normalize_server("ftp://example"), None);
        assert_eq!(normalize_server(""), None);
    }
}
