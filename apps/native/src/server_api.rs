//! The few server calls the app makes itself, from
//! Rust with the credential as a bearer token: never from the web view, so
//! no origin needs allowing.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The app's client kind on servers.
pub const KIND: &str = "app";

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0} does not answer: {1}")]
    Unreachable(String, String),
    #[error("{0} is not an Ahab server")]
    NotAhab(String),
    /// A Problem Details answer: its stable code and what it says.
    #[error("{message}")]
    Problem { code: String, message: String },
}

impl ApiError {
    pub fn code(&self) -> &str {
        match self {
            Self::Unreachable(..) => "unreachable",
            Self::NotAhab(_) => "not_ahab",
            Self::Problem { code, .. } => code,
        }
    }
}

/// What `GET /api/v1/health` says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub version: String,
}

/// A client just paired: its ID and credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paired {
    pub client_id: Option<String>,
    pub credential: String,
}

/// A request to be paired, waiting for approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingRequest {
    pub id: String,
    pub code: String,
    pub secret: String,
}

/// A join code for this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinCode {
    pub code: String,
    /// The server's key fingerprint; absent from older servers.
    pub fingerprint: Option<String>,
}

/// The answer to a request so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Waiting,
    Denied,
    Expired,
    Paired(Paired),
}

/// This server's own machine and its agents (`local_agents`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAgents {
    /// Its name, such as `thinkpad`; `None` before the machine is there.
    pub machine: Option<String>,
    pub online: bool,
    pub agents: Vec<FoundAgent>,
}

/// An agent a machine knows, as the checklist shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundAgent {
    pub agent: String,
    pub label: String,
    /// `ready`, `notSignedIn`, `notInstalled`… (the API's agent states).
    pub state: String,
}

/// The machine marked as this server's own (`isLocal`).
fn local_machine(machines: &Value) -> Option<&Value> {
    machines["items"]
        .as_array()?
        .iter()
        .find(|machine| machine["isLocal"].as_bool() == Some(true))
}

fn agents_of(agents: &Value) -> Vec<FoundAgent> {
    agents["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let agent = item["agent"].as_str()?.to_owned();
                    Some(FoundAgent {
                        label: item["label"].as_str().unwrap_or(&agent).to_owned(),
                        state: item["state"].as_str().unwrap_or("error").to_owned(),
                        agent,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone)]
pub struct ServerApi {
    http: reqwest::Client,
    url: String,
}

impl ServerApi {
    pub fn new(url: &str) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent(format!("Ahab app {}", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self {
            http,
            url: url.trim_end_matches('/').to_owned(),
        }
    }

    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        credential: Option<&str>,
        body: Option<Value>,
        timeout: Duration,
    ) -> Result<Value, ApiError> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.url))
            .timeout(timeout);
        if let Some(credential) = credential {
            request = request.bearer_auth(credential);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| ApiError::Unreachable(self.url.clone(), describe(&error)))?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let value: Value = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).map_err(|_| ApiError::NotAhab(self.url.clone()))?
        };
        if status.is_success() {
            return Ok(value);
        }
        let code = value["code"].as_str().unwrap_or("unexpected").to_owned();
        let message = value["detail"]
            .as_str()
            .or_else(|| value["title"].as_str())
            .map_or_else(|| format!("The server answered {status}."), str::to_owned);
        Err(ApiError::Problem { code, message })
    }

    #[cfg(desktop)]
    pub async fn notification_stream(
        &self,
        credential: &str,
    ) -> reqwest::Result<reqwest::Response> {
        self.http
            .get(format!("{}/api/v1/notification-events", self.url))
            .bearer_auth(credential)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await?
            .error_for_status()
    }

    #[cfg(desktop)]
    pub async fn unread_notifications(&self, credential: &str) -> Option<u32> {
        let value = self
            .send(
                reqwest::Method::GET,
                "/api/v1/notifications?limit=1",
                Some(credential),
                None,
                Duration::from_secs(10),
            )
            .await
            .ok()?;
        Some(u32::try_from(value["unread"].as_u64()?).unwrap_or(u32::MAX))
    }

    #[cfg(desktop)]
    pub async fn notification_allowed(&self, credential: &str, kind: &str) -> bool {
        let Ok(settings) = self
            .send(
                reqwest::Method::GET,
                "/api/v1/notification-settings",
                Some(credential),
                None,
                Duration::from_secs(10),
            )
            .await
        else {
            return false;
        };
        settings["outside"].as_bool().unwrap_or(true)
            && settings["kinds"]
                .as_array()
                .and_then(|kinds| kinds.iter().find(|entry| entry["id"] == kind))
                .and_then(|entry| entry["outside"].as_bool())
                .unwrap_or(true)
    }

    /// Whether it is an Ahab server, and its version.
    pub async fn health(&self) -> Result<Health, ApiError> {
        let value = self
            .send(
                reqwest::Method::GET,
                "/api/v1/health",
                None,
                None,
                Duration::from_secs(10),
            )
            .await?;
        match (value["status"].as_str(), value["version"].as_str()) {
            (Some("ok"), Some(version)) => Ok(Health {
                version: version.to_owned(),
            }),
            _ => Err(ApiError::NotAhab(self.url.clone())),
        }
    }

    /// Pairs with a link's secret or a code from Add client.
    pub async fn pair(
        &self,
        secret: Option<&str>,
        code: Option<&str>,
        name: &str,
    ) -> Result<Paired, ApiError> {
        let mut body = json!({ "name": name, "kind": KIND });
        if let Some(secret) = secret {
            body["secret"] = json!(secret);
        }
        if let Some(code) = code {
            body["code"] = json!(code);
        }
        let value = self
            .send(
                reqwest::Method::POST,
                "/api/v1/pair",
                None,
                Some(body),
                Duration::from_secs(20),
            )
            .await?;
        paired(&value).ok_or_else(|| ApiError::NotAhab(self.url.clone()))
    }

    /// Asks to be paired; a paired client approves the code.
    pub async fn request_pairing(&self, name: &str) -> Result<PairingRequest, ApiError> {
        let value = self
            .send(
                reqwest::Method::POST,
                "/api/v1/pairing-requests",
                None,
                Some(json!({ "name": name, "kind": KIND })),
                Duration::from_secs(20),
            )
            .await?;
        serde_json::from_value(value).map_err(|_| ApiError::NotAhab(self.url.clone()))
    }

    /// Waits a while for the answer to a request (the server holds the
    /// call until something changes or it times out).
    pub async fn wait_for_answer(&self, request: &PairingRequest) -> Result<Answer, ApiError> {
        let secret: String =
            url::form_urlencoded::byte_serialize(request.secret.as_bytes()).collect();
        let path = format!("/api/v1/pairing-requests/{}?secret={secret}", request.id);
        let value = match self
            .send(
                reqwest::Method::GET,
                &path,
                None,
                None,
                Duration::from_secs(70),
            )
            .await
        {
            Ok(value) => value,
            Err(ApiError::Problem { code, .. }) if code == "pairing_expired" => {
                return Ok(Answer::Expired);
            }
            Err(error) => return Err(error),
        };
        Ok(match value["state"].as_str() {
            Some("used" | "approved") => match paired(&value) {
                Some(paired) => Answer::Paired(paired),
                None => Answer::Waiting,
            },
            Some("denied") => Answer::Denied,
            Some("expired") => Answer::Expired,
            _ => Answer::Waiting,
        })
    }

    /// A one-time pairing link, made with a credential (the local one).
    pub async fn create_link(&self, credential: &str) -> Result<String, ApiError> {
        let value = self
            .send(
                reqwest::Method::POST,
                "/api/v1/pairings",
                Some(credential),
                Some(json!({})),
                Duration::from_secs(20),
            )
            .await?;
        value["secret"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| ApiError::NotAhab(self.url.clone()))
    }

    /// A one-time join code for this computer, with the server's key
    /// fingerprint to pin (as `ahab machines add you@host` does).
    pub async fn create_join_code(
        &self,
        credential: &str,
        name: Option<&str>,
    ) -> Result<JoinCode, ApiError> {
        let value = self
            .send(
                reqwest::Method::POST,
                "/api/v1/join-codes",
                Some(credential),
                Some(json!({ "name": name })),
                Duration::from_secs(20),
            )
            .await?;
        Ok(JoinCode {
            code: value["code"]
                .as_str()
                .ok_or_else(|| ApiError::NotAhab(self.url.clone()))?
                .to_owned(),
            fingerprint: value["serverFingerprint"].as_str().map(str::to_owned),
        })
    }

    /// The machine this server runs a daemon on (its own computer), and the
    /// agents that machine found: what the first launch's checklist shows.
    pub async fn local_agents(&self, credential: &str) -> Result<LocalAgents, ApiError> {
        let machines = self
            .send(
                reqwest::Method::GET,
                "/api/v1/machines",
                Some(credential),
                None,
                Duration::from_secs(10),
            )
            .await?;
        let Some(machine) = local_machine(&machines) else {
            return Ok(LocalAgents::default());
        };
        let id = machine["id"].as_str().unwrap_or_default();
        let agents = if machine["online"].as_bool() == Some(true) {
            self.send(
                reqwest::Method::GET,
                &format!("/api/v1/machines/{id}/agents"),
                Some(credential),
                None,
                Duration::from_secs(30),
            )
            .await?
        } else {
            Value::Null
        };
        Ok(LocalAgents {
            machine: machine["name"].as_str().map(str::to_owned),
            online: machine["online"].as_bool() == Some(true),
            agents: agents_of(&agents),
        })
    }

    /// The client a credential belongs to, if it is still paired.
    pub async fn current_client(&self, credential: &str) -> Result<Value, ApiError> {
        self.send(
            reqwest::Method::GET,
            "/api/v1/clients/current",
            Some(credential),
            None,
            Duration::from_secs(10),
        )
        .await
    }

    /// Revokes the app's own client on the server.
    pub async fn revoke(&self, credential: &str, client_id: &str) -> Result<(), ApiError> {
        self.send(
            reqwest::Method::DELETE,
            &format!("/api/v1/clients/{client_id}"),
            Some(credential),
            None,
            Duration::from_secs(10),
        )
        .await
        .map(|_| ())
    }
}

fn paired(value: &Value) -> Option<Paired> {
    Some(Paired {
        credential: value["credential"].as_str()?.to_owned(),
        client_id: value["client"]["id"].as_str().map(str::to_owned),
    })
}

fn describe(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "it took too long".to_owned()
    } else if error.is_connect() {
        "nothing answers at that address".to_owned()
    } else {
        error.to_string()
    }
}

/// The name this app's client gets on servers.
pub fn client_name() -> String {
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    if host.is_empty() {
        "Ahab app".to_owned()
    } else {
        format!("Ahab app on {host}")
    }
}

#[cfg(test)]
mod local_agents {
    use serde_json::json;

    use super::{agents_of, local_machine};

    #[test]
    fn finds_this_servers_machine_and_its_agents() {
        let machines = json!({ "items": [
            { "id": "a", "name": "build-box", "isLocal": false, "online": true },
            { "id": "b", "name": "thinkpad", "isLocal": true, "online": true },
        ]});
        assert_eq!(local_machine(&machines).unwrap()["name"], "thinkpad");
        assert!(local_machine(&json!({ "items": [] })).is_none());
        let agents = agents_of(&json!({ "items": [
            { "agent": "claude-code", "label": "Claude Code", "state": "ready" },
            { "agent": "pi", "state": "notInstalled" },
            { "label": "no id" },
        ]}));
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].label, "Claude Code");
        assert_eq!(agents[1].label, "pi");
        assert_eq!(agents[1].state, "notInstalled");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_paired_answer() {
        let value = json!({ "client": { "id": "d1" }, "credential": "ahab_x" });
        assert_eq!(
            paired(&value),
            Some(Paired {
                client_id: Some("d1".into()),
                credential: "ahab_x".into()
            })
        );
        assert_eq!(paired(&json!({ "client": { "id": "d1" } })), None);
    }

    #[tokio::test]
    async fn says_when_nothing_answers() {
        // A port nothing listens on, on loopback.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let error = ServerApi::new(&format!("http://127.0.0.1:{port}"))
            .health()
            .await
            .unwrap_err();
        assert_eq!(error.code(), "unreachable");
    }
}
