//! The daemon's side of `agents.usage`: the plan limits of the Claude Code
//! and Codex accounts signed in on this computer (how much of the 5-hour
//! session and the week is used, and when each starts over).
//!
//! Each agent's own sign-in is read where the agent keeps it and sent only
//! to that agent's provider; it never leaves this computer otherwise, and is
//! never refreshed or written. The providers' usage endpoints are not
//! documented and limit how often they may be asked, so each agent is asked
//! at most once a minute, and not again before a `Retry-After` has passed:
//! until then the last report stands.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc2822;

use crate::modules::machines::agents::model::label;
use crate::modules::machines::usage::api::{AgentUsageDto, AgentUsageListDto, UsageStatusDto};
use crate::modules::machines::usage::model::{
    claude_plan, claude_windows, codex_plan, codex_windows, jwt_expiry,
};

/// How long one provider may take to answer.
const TIMEOUT: Duration = Duration::from_secs(15);
/// The least time between two questions to one provider.
const MIN_INTERVAL: Duration = Duration::from_secs(60);
/// How long to wait after a 429 that names no time.
const DEFAULT_BACKOFF: Duration = Duration::from_mins(5);

const CLAUDE_USAGE: &str = "https://api.anthropic.com/api/oauth/usage";
const CLAUDE_OAUTH_BETA: &str = "oauth-2025-04-20";
const CODEX_USAGE: &str = "https://chatgpt.com/backend-api/wham/usage";

/// `agents.usage`: Claude Code's and Codex's limits, asked side by side.
pub async fn usage() -> AgentUsageListDto {
    let (claude, codex) = tokio::join!(
        cached("claude-code", || claude_usage(&CLIENT)),
        cached("codex", || codex_usage(&CLIENT)),
    );
    AgentUsageListDto {
        items: vec![claude, codex],
    }
}

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .unwrap_or_default()
});

/// What asking a provider gave.
enum Fetched {
    Report(AgentUsageDto),
    /// 429: not again before this long (when the provider said).
    RateLimited(Option<Duration>),
}

struct Entry {
    asked: Instant,
    /// The last report (a good one stays through rate limits).
    report: AgentUsageDto,
    not_before: Option<Instant>,
}

static CACHE: LazyLock<Mutex<HashMap<&'static str, Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cache() -> std::sync::MutexGuard<'static, HashMap<&'static str, Entry>> {
    CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The agent's report: the last one while it is recent or the provider asked
/// to wait, else a new one.
async fn cached<F, Fut>(agent: &'static str, fetch: F) -> AgentUsageDto
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Fetched>,
{
    let now = Instant::now();
    if let Some(entry) = cache().get(agent) {
        let waiting = entry.not_before.is_some_and(|until| until > now);
        if waiting || now.duration_since(entry.asked) < MIN_INTERVAL {
            return entry.report.clone();
        }
    }
    let fetched = fetch().await;
    let mut cache = cache();
    let (report, not_before) = match fetched {
        Fetched::Report(report) => (report, None),
        Fetched::RateLimited(wait) => {
            let wait = wait.unwrap_or(DEFAULT_BACKOFF);
            let problem = format!(
                "{} limits how often its usage may be read; asking again in {}.",
                provider(agent),
                minutes(wait)
            );
            let report = match cache.get(agent) {
                // The last good figures stand, saying why they are not newer.
                Some(entry) if entry.report.status == UsageStatusDto::Available => AgentUsageDto {
                    problem: Some(problem),
                    ..entry.report.clone()
                },
                _ => report(agent, UsageStatusDto::Error, problem),
            };
            (report, Some(now + wait))
        }
    };
    cache.insert(
        agent,
        Entry {
            asked: now,
            report: report.clone(),
            not_before,
        },
    );
    report
}

fn minutes(wait: Duration) -> String {
    let minutes = wait.as_secs().div_ceil(60).max(1);
    if minutes == 1 {
        "1 minute".to_owned()
    } else {
        format!("{minutes} minutes")
    }
}

fn provider(agent: &str) -> &'static str {
    match agent {
        "claude-code" => "Anthropic",
        _ => "OpenAI",
    }
}

fn report(agent: &str, status: UsageStatusDto, problem: impl Into<String>) -> AgentUsageDto {
    AgentUsageDto {
        agent: agent.to_owned(),
        label: label(agent).to_owned(),
        status,
        plan: None,
        windows: Vec::new(),
        problem: Some(problem.into()),
        fetched_at: OffsetDateTime::now_utc(),
    }
}

fn home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
}

/// A `Retry-After` header: seconds, or an HTTP date.
fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    let value = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?;
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let at = OffsetDateTime::parse(value.trim(), &Rfc2822).ok()?;
    Duration::try_from(at - OffsetDateTime::now_utc()).ok()
}

// Claude Code ---------------------------------------------------------------

/// Claude Code's sign-in: its config folder's `.credentials.json`, or the
/// macOS Keychain, where Claude Code keeps it there.
async fn claude_credentials() -> Option<Value> {
    let folder = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| home().map(|home| home.join(".claude")))?;
    if let Ok(text) = tokio::fs::read_to_string(folder.join(".credentials.json")).await {
        return serde_json::from_str(&text).ok();
    }
    if cfg!(target_os = "macos") {
        let output = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::process::Command::new("security")
                .args([
                    "find-generic-password",
                    "-s",
                    "Claude Code-credentials",
                    "-w",
                ])
                .kill_on_drop(true)
                .output(),
        )
        .await
        .ok()?
        .ok()?;
        if output.status.success() {
            return serde_json::from_slice(&output.stdout).ok();
        }
    }
    None
}

async fn claude_usage(client: &reqwest::Client) -> Fetched {
    const AGENT: &str = "claude-code";
    let Some(oauth) = claude_credentials()
        .await
        .and_then(|credentials| credentials.get("claudeAiOauth").cloned())
    else {
        return Fetched::Report(report(
            AGENT,
            UsageStatusDto::Unavailable,
            "Not signed in with a Claude subscription here. An API key has no plan limits.",
        ));
    };
    let Some(token) = oauth.get("accessToken").and_then(Value::as_str) else {
        return Fetched::Report(report(
            AGENT,
            UsageStatusDto::Unavailable,
            "Not signed in with a Claude subscription here.",
        ));
    };
    let plan = claude_plan(
        oauth.get("subscriptionType").and_then(Value::as_str),
        oauth.get("rateLimitTier").and_then(Value::as_str),
    );
    if let Some(expires) = oauth.get("expiresAt").and_then(Value::as_i64)
        && i128::from(expires) <= OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000
    {
        return Fetched::Report(AgentUsageDto {
            plan,
            ..report(
                AGENT,
                UsageStatusDto::Unavailable,
                "The saved sign-in has expired: Claude Code renews it the next time it runs.",
            )
        });
    }
    let answer = client
        .get(CLAUDE_USAGE)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .header("anthropic-beta", CLAUDE_OAUTH_BETA)
        .send()
        .await;
    let response = match answer {
        Ok(response) => response,
        Err(error) => {
            return Fetched::Report(report(
                AGENT,
                UsageStatusDto::Error,
                format!("Anthropic cannot be reached: {error}"),
            ));
        }
    };
    match response.status() {
        StatusCode::TOO_MANY_REQUESTS => return Fetched::RateLimited(retry_after(&response)),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            return Fetched::Report(AgentUsageDto {
                plan,
                ..report(
                    AGENT,
                    UsageStatusDto::Unavailable,
                    "Anthropic did not accept the saved sign-in: Claude Code renews it the next time it runs, or run `claude auth login`.",
                )
            });
        }
        status if !status.is_success() => {
            return Fetched::Report(report(
                AGENT,
                UsageStatusDto::Error,
                format!("Anthropic answered {status}."),
            ));
        }
        _ => {}
    }
    match response.json::<Value>().await {
        Ok(body) => Fetched::Report(AgentUsageDto {
            agent: AGENT.to_owned(),
            label: label(AGENT).to_owned(),
            status: UsageStatusDto::Available,
            plan,
            windows: claude_windows(&body),
            problem: None,
            fetched_at: OffsetDateTime::now_utc(),
        }),
        Err(error) => Fetched::Report(report(
            AGENT,
            UsageStatusDto::Error,
            format!("Anthropic's answer is unreadable: {error}"),
        )),
    }
}

// Codex ---------------------------------------------------------------------

/// Codex's sign-in: `$CODEX_HOME/auth.json`, by default `~/.codex/auth.json`.
async fn codex_auth() -> Option<Value> {
    let folder = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| home().map(|home| home.join(".codex")))?;
    let text = tokio::fs::read_to_string(folder.join("auth.json"))
        .await
        .ok()?;
    serde_json::from_str(&text).ok()
}

async fn codex_usage(client: &reqwest::Client) -> Fetched {
    const AGENT: &str = "codex";
    let auth = codex_auth().await;
    let tokens = auth.as_ref().and_then(|auth| auth.get("tokens"));
    let Some(token) = tokens
        .and_then(|tokens| tokens.get("access_token"))
        .and_then(Value::as_str)
    else {
        return Fetched::Report(report(
            AGENT,
            UsageStatusDto::Unavailable,
            "Not signed in with a ChatGPT plan here. An API key has no plan limits.",
        ));
    };
    if let Some(expires) = jwt_expiry(token)
        && expires <= OffsetDateTime::now_utc()
    {
        return Fetched::Report(report(
            AGENT,
            UsageStatusDto::Unavailable,
            "The saved sign-in has expired: Codex renews it the next time it runs.",
        ));
    }
    let mut request = client
        .get(CODEX_USAGE)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        // The endpoint serves browsers and the Codex app; others get a page.
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        );
    if let Some(account) = tokens
        .and_then(|tokens| tokens.get("account_id"))
        .and_then(Value::as_str)
    {
        request = request.header("ChatGPT-Account-Id", account);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            return Fetched::Report(report(
                AGENT,
                UsageStatusDto::Error,
                format!("OpenAI cannot be reached: {error}"),
            ));
        }
    };
    match response.status() {
        StatusCode::TOO_MANY_REQUESTS => return Fetched::RateLimited(retry_after(&response)),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            return Fetched::Report(report(
                AGENT,
                UsageStatusDto::Unavailable,
                "OpenAI did not accept the saved sign-in: Codex renews it the next time it runs, or run `codex login`.",
            ));
        }
        status if !status.is_success() => {
            return Fetched::Report(report(
                AGENT,
                UsageStatusDto::Error,
                format!("OpenAI answered {status}."),
            ));
        }
        _ => {}
    }
    let text = response.text().await.unwrap_or_default();
    match serde_json::from_str::<Value>(&text) {
        Ok(body) => Fetched::Report(AgentUsageDto {
            agent: AGENT.to_owned(),
            label: label(AGENT).to_owned(),
            status: UsageStatusDto::Available,
            plan: body
                .get("plan_type")
                .and_then(Value::as_str)
                .map(codex_plan),
            windows: codex_windows(&body),
            problem: None,
            fetched_at: OffsetDateTime::now_utc(),
        }),
        Err(_) => Fetched::Report(report(
            AGENT,
            UsageStatusDto::Error,
            "OpenAI's answer is unreadable.",
        )),
    }
}
