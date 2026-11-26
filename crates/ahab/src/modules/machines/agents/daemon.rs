//! The daemon's side: `agents.detect`, answered through Stubb's probe. It only
//! runs status commands; it never starts an agent or sends a prompt.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::Duration;

use stubb::discovery::update::{InstallMethod, Package};
use stubb::discovery::{Auth, Report};
use time::OffsetDateTime;

use crate::modules::machines::agents::api::{
    AddedToDetect, AgentStateDto, AgentUpdateDto, AgentUpdateListDto, AgentUpdatedDto,
    DetectRequest, Detected, DetectedAgent,
};
use crate::modules::machines::agents::model::{agent_id, harness, label};
use crate::shared::error::{DaemonError, fail};

/// How long an added agent may take to answer `initialize` when detected
/// (an `npx` agent may first download its package).
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(60);

/// The added agents this daemon was last told about: their name and how to
/// start them, for sessions.
static ADDED: LazyLock<Mutex<HashMap<String, (String, stubb::providers::acp::Launch)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// How to start an added agent (`acp:…`, `custom:…`), as last detected.
pub fn added_launch(agent: &str) -> Option<(String, stubb::providers::acp::Launch)> {
    ADDED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(agent)
        .cloned()
}

/// Detects every agent Stubb supports on this computer, and the added ones
/// (over ACP: found, and answering `initialize`).
pub async fn detect(request: DetectRequest) -> Detected {
    let (native, added) = tokio::join!(
        stubb::discovery::probe_all(),
        futures_util::future::join_all(request.added.into_iter().map(detect_added)),
    );
    let mut agents: Vec<DetectedAgent> = native.into_iter().map(to_detected).collect();
    agents.extend(added);
    Detected { agents }
}

async fn detect_added(added: AddedToDetect) -> DetectedAgent {
    let missing = |detail: String| DetectedAgent {
        agent: added.agent.clone(),
        label: Some(added.name.clone()),
        state: AgentStateDto::NotInstalled,
        version: None,
        path: None,
        account: None,
        detail: Some(detail),
    };
    let launch = match (&added.launch, &added.entry) {
        (Some(custom), _) => {
            let mut launch =
                stubb::providers::acp::Launch::new(&custom.program).args(custom.args.clone());
            for (name, value) in &custom.env {
                launch = launch.env(name, value);
            }
            launch
        }
        (None, Some(entry)) => {
            let entry = match serde_json::from_value::<stubb::providers::acp::registry::Entry>(
                entry.clone(),
            ) {
                Ok(entry) => entry,
                Err(error) => return missing(format!("Its registry entry is unreadable: {error}")),
            };
            match entry.resolve(None).await {
                Ok(launch) => launch,
                Err(error) => return missing(error.to_string()),
            }
        }
        (None, None) => return missing("Ahab knows no way to start it.".to_owned()),
    };
    ADDED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(added.agent.clone(), (added.name.clone(), launch.clone()));
    let report = stubb::providers::acp::probe(
        &launch,
        &stubb::process::Environment::inherit(),
        Some(INITIALIZE_TIMEOUT),
    )
    .await;
    let state = if !report.installed() {
        AgentStateDto::NotInstalled
    } else if report.initialized {
        AgentStateDto::Ready
    } else {
        AgentStateDto::Error
    };
    DetectedAgent {
        agent: added.agent,
        label: Some(added.name),
        state,
        version: report.agent.map(|agent| agent.version),
        path: report.path.map(|path| path.display().to_string()),
        account: None,
        // ACP cannot tell whether an agent is signed in; say how it signs in.
        detail: report.detail.or_else(|| {
            (!report.auth_methods.is_empty()).then(|| {
                let ways: Vec<&str> = report
                    .auth_methods
                    .iter()
                    .map(|method| method.name.as_str())
                    .collect();
                format!(
                    "It starts; whether it is signed in shows when a session starts. It signs in with: {}.",
                    ways.join(", ")
                )
            })
        }),
    }
}

fn to_detected(report: Report) -> DetectedAgent {
    let state = match (&report.path, &report.auth) {
        (None, _) => AgentStateDto::NotInstalled,
        (Some(_), Auth::SignedIn { .. }) => AgentStateDto::Ready,
        (Some(_), Auth::SignedOut) => AgentStateDto::NotSignedIn,
        // Unknown, or a state a newer Stubb adds: Ahab cannot tell.
        (Some(_), _) => AgentStateDto::Error,
    };
    let account = match &report.auth {
        Auth::SignedIn { account } => account.clone(),
        _ => None,
    };
    DetectedAgent {
        agent: agent_id(report.cli).to_owned(),
        label: None,
        state,
        version: report.version,
        path: report.path.map(|path| path.display().to_string()),
        account,
        detail: match state {
            // A missing agent needs no explanation beyond its state.
            AgentStateDto::NotInstalled => None,
            _ => report.detail,
        },
    }
}

/// `agents.updates`: for each installed agent, the newest release on its
/// channel (from npm) and whether Ahab can install it here.
pub async fn updates() -> AgentUpdateListDto {
    let reports = stubb::discovery::probe_all().await;
    let checks = reports
        .iter()
        .filter(|report| report.installed())
        .map(|report| async move {
            let plan = stubb::discovery::update::plan(report);
            let latest = latest(&plan.package).await;
            let agent = agent_id(report.cli);
            let (latest, lookup) = match latest {
                Ok(version) => (Some(version), None),
                Err(why) => (None, Some(why)),
            };
            AgentUpdateDto {
                agent: agent.to_owned(),
                label: label(agent).to_owned(),
                available: match (&latest, &report.version) {
                    (Some(latest), Some(installed)) => {
                        stubb::discovery::update::is_newer(latest, installed)
                    }
                    _ => false,
                },
                installed: report.version.clone(),
                latest,
                channel: plan.package.tag.clone(),
                method: method_name(&plan.method).to_owned(),
                can_update: plan.can_update,
                command: plan.command,
                reason: plan.reason.or(lookup),
                checked_at: OffsetDateTime::now_utc(),
            }
        });
    AgentUpdateListDto {
        items: futures_util::future::join_all(checks).await,
    }
}

/// `agents.update`: updates one agent with its own updater, then reads its
/// version again.
pub async fn update(agent: &str) -> Result<AgentUpdatedDto, DaemonError> {
    let Some(harness) = harness(agent) else {
        return Err(fail(
            "not_found",
            format!("Ahab does not know the agent {agent}."),
        ));
    };
    let report = stubb::discovery::probe(harness.cli(), None).await;
    let outcome = stubb::discovery::update::update(&report)
        .await
        .map_err(|error| fail("update_failed", error.to_string()))?;
    Ok(AgentUpdatedDto {
        agent: agent.to_owned(),
        before: outcome.before,
        after: outcome.after,
        success: outcome.success,
        output: outcome.output,
    })
}

static REGISTRY: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default()
});

/// The version `package`'s channel points at, from the npm registry.
async fn latest(package: &Package) -> Result<String, String> {
    let response = REGISTRY
        .get(package.registry_url())
        // The abbreviated document: dist-tags without every version's details.
        .header(
            reqwest::header::ACCEPT,
            "application/vnd.npm.install-v1+json",
        )
        .send()
        .await
        .map_err(|error| format!("npm cannot be reached: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("npm answered {}", response.status()));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("npm's answer is unreadable: {error}"))?;
    body.get("dist-tags")
        .and_then(|tags| tags.get(&package.tag))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("npm names no `{}` release of {}", package.tag, package.npm))
}

fn method_name(method: &InstallMethod) -> &'static str {
    match method {
        InstallMethod::Native => "native",
        InstallMethod::Npm { .. } => "npm",
        InstallMethod::Homebrew => "homebrew",
        InstallMethod::Nix => "nix",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::modules::machines::Harness;

    use super::*;

    fn report(path: Option<&str>, auth: Auth) -> Report {
        let mut report = Report::new(Harness::Claude.cli());
        report.path = path.map(PathBuf::from);
        report.version = Some("2.1.322".into());
        report.auth = auth;
        report.detail = Some("why".into());
        report
    }

    #[test]
    fn readiness_follows_the_probe() {
        let ready = to_detected(report(
            Some("/bin/claude"),
            Auth::SignedIn {
                account: Some("a@example.com".into()),
            },
        ));
        assert_eq!(ready.agent, "claude-code");
        assert_eq!(ready.state, AgentStateDto::Ready);
        assert_eq!(ready.account.as_deref(), Some("a@example.com"));
        assert_eq!(
            to_detected(report(Some("/bin/claude"), Auth::SignedOut)).state,
            AgentStateDto::NotSignedIn
        );
        assert_eq!(
            to_detected(report(Some("/bin/claude"), Auth::Unknown)).state,
            AgentStateDto::Error
        );
        let missing = to_detected(report(None, Auth::Unknown));
        assert_eq!(missing.state, AgentStateDto::NotInstalled);
        assert_eq!(missing.detail, None);
    }
}
