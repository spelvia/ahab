//! Reading the providers' answers about plan limits: pure functions from
//! their JSON to Ahab's limits, used by the daemon (`daemon.rs`).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;
use time::OffsetDateTime;

use crate::modules::machines::usage::api::UsageWindowDto;
use crate::shared::time::rfc3339;

/// "Max 5x" from `max` and `default_claude_max_5x`; "Pro" from `pro`.
pub fn claude_plan(subscription: Option<&str>, tier: Option<&str>) -> Option<String> {
    let mut plan = capitalized(subscription?);
    let multiple = tier
        .and_then(|tier| tier.rsplit('_').next())
        .filter(|last| {
            last.len() > 1
                && last.ends_with('x')
                && last[..last.len() - 1].chars().all(|c| c.is_ascii_digit())
        });
    if let Some(multiple) = multiple {
        plan.push(' ');
        plan.push_str(multiple);
    }
    Some(plan)
}

/// The limits in Anthropic's answer: its `limits` list, or, in answers
/// without one, the `five_hour` and `seven_day…` fields.
pub fn claude_windows(body: &Value) -> Vec<UsageWindowDto> {
    let listed: Vec<UsageWindowDto> = body
        .get("limits")
        .and_then(Value::as_array)
        .map(|limits| limits.iter().filter_map(claude_limit).collect())
        .unwrap_or_default();
    if !listed.is_empty() {
        return listed;
    }
    let field = |name: &str| -> Option<(f64, Option<OffsetDateTime>)> {
        let window = body.get(name)?;
        let used = number(window.get("utilization")?)?;
        Some((used, window.get("resets_at").and_then(time)))
    };
    let mut windows = Vec::new();
    if let Some((used, resets)) = field("five_hour") {
        windows.push(session(used, resets));
    }
    if let Some((used, resets)) = field("seven_day") {
        windows.push(weekly(used, resets));
    }
    for (name, model) in [("seven_day_opus", "Opus"), ("seven_day_sonnet", "Sonnet")] {
        if let Some((used, resets)) = field(name) {
            windows.push(scoped_weekly(model, used, resets));
        }
    }
    windows
}

fn claude_limit(limit: &Value) -> Option<UsageWindowDto> {
    let used = number(limit.get("percent")?)?;
    let resets = limit.get("resets_at").and_then(time);
    match limit.get("kind").and_then(Value::as_str)? {
        "session" => Some(session(used, resets)),
        "weekly_all" => Some(weekly(used, resets)),
        "weekly_scoped" => {
            let scope = limit.get("scope")?;
            let name = ["model", "surface"].iter().find_map(|dimension| {
                let named = scope.get(dimension)?;
                named
                    .get("display_name")
                    .and_then(Value::as_str)
                    .or_else(|| named.get("id").and_then(Value::as_str))
            })?;
            Some(scoped_weekly(name, used, resets))
        }
        other => Some(UsageWindowDto {
            id: other.to_owned(),
            label: capitalized(&other.replace('_', " ")),
            short_label: None,
            used_percent: used,
            resets_at: resets,
        }),
    }
}

fn session(used: f64, resets: Option<OffsetDateTime>) -> UsageWindowDto {
    UsageWindowDto {
        id: "session".to_owned(),
        label: "Session (5 hours)".to_owned(),
        short_label: Some("5h".to_owned()),
        used_percent: used,
        resets_at: resets,
    }
}

fn weekly(used: f64, resets: Option<OffsetDateTime>) -> UsageWindowDto {
    UsageWindowDto {
        id: "weekly".to_owned(),
        label: "Weekly".to_owned(),
        short_label: Some("wk".to_owned()),
        used_percent: used,
        resets_at: resets,
    }
}

fn scoped_weekly(name: &str, used: f64, resets: Option<OffsetDateTime>) -> UsageWindowDto {
    UsageWindowDto {
        id: format!("weekly:{}", name.to_lowercase().replace(' ', "-")),
        label: format!("Weekly · {name}"),
        short_label: None,
        used_percent: used,
        resets_at: resets,
    }
}

/// When a JWT expires, from its `exp` claim (read, not verified).
pub fn jwt_expiry(token: &str) -> Option<OffsetDateTime> {
    let payload = token.split('.').nth(1)?;
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;
    OffsetDateTime::from_unix_timestamp(claims.get("exp")?.as_i64()?).ok()
}

pub fn codex_plan(plan: &str) -> String {
    match plan {
        "prolite" => "Pro Lite".to_owned(),
        "edu" => "Edu".to_owned(),
        other => capitalized(other),
    }
}

/// The limits in the Codex answer: the plan's own, then any others (by
/// model or feature) and code review's.
pub fn codex_windows(body: &Value) -> Vec<UsageWindowDto> {
    let mut windows = codex_pair(body.get("rate_limit"), None);
    if let Some(others) = body.get("additional_rate_limits").and_then(Value::as_array) {
        for other in others {
            let name = other
                .get("limit_name")
                .and_then(Value::as_str)
                .or_else(|| other.get("metered_feature").and_then(Value::as_str));
            if let Some(name) = name {
                windows.extend(codex_pair(other.get("rate_limit"), Some(name)));
            }
        }
    }
    windows.extend(codex_pair(
        body.get("code_review_rate_limit"),
        Some("Code review"),
    ));
    windows
}

fn codex_pair(limit: Option<&Value>, scope: Option<&str>) -> Vec<UsageWindowDto> {
    let Some(limit) = limit.filter(|limit| limit.is_object()) else {
        return Vec::new();
    };
    ["primary_window", "secondary_window"]
        .iter()
        .filter_map(|slot| {
            let window = limit.get(*slot).filter(|window| window.is_object())?;
            let used = number(window.get("used_percent")?)?;
            let resets = window
                .get("reset_at")
                .and_then(Value::as_i64)
                .and_then(|at| OffsetDateTime::from_unix_timestamp(at).ok());
            let seconds = window.get("limit_window_seconds").and_then(Value::as_u64);
            let mut found = match seconds {
                Some(18_000) => session(used, resets),
                Some(604_800) => weekly(used, resets),
                Some(seconds) if seconds > 0 => {
                    let (amount, unit, short) = if seconds % 86_400 == 0 {
                        (seconds / 86_400, "day", "d")
                    } else {
                        (seconds.div_ceil(3_600), "hour", "h")
                    };
                    let plural = if amount == 1 { "" } else { "s" };
                    UsageWindowDto {
                        id: format!("window:{seconds}"),
                        label: format!("{amount} {unit}{plural}"),
                        short_label: Some(format!("{amount}{short}")),
                        used_percent: used,
                        resets_at: resets,
                    }
                }
                _ => UsageWindowDto {
                    id: (*slot).trim_end_matches("_window").to_owned(),
                    label: if *slot == "primary_window" {
                        "Main limit".to_owned()
                    } else {
                        "Second limit".to_owned()
                    },
                    short_label: None,
                    used_percent: used,
                    resets_at: resets,
                },
            };
            if let Some(scope) = scope {
                found.id = format!("{}:{}", scope.to_lowercase().replace(' ', "-"), found.id);
                found.label = format!("{} · {scope}", found.label);
                found.short_label = None;
            }
            Some(found)
        })
        .collect()
}

// Shared --------------------------------------------------------------------

/// A number, also when sent as a string.
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
    .filter(|number: &f64| number.is_finite())
}

fn time(value: &Value) -> Option<OffsetDateTime> {
    rfc3339::parse(value.as_str()?).ok()
}

pub fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn claude_plans_read_as_people_name_them() {
        assert_eq!(
            claude_plan(Some("max"), Some("default_claude_max_5x")).as_deref(),
            Some("Max 5x")
        );
        assert_eq!(
            claude_plan(Some("pro"), Some("default_claude_pro")).as_deref(),
            Some("Pro")
        );
        assert_eq!(claude_plan(None, Some("default_claude_max_5x")), None);
    }

    #[test]
    fn claude_limits_come_from_its_list() {
        let body = json!({
            "five_hour": { "utilization": 1.0, "resets_at": null },
            "limits": [
                { "kind": "session", "percent": 93, "resets_at": "2026-11-16T17:40:00.523816+00:00", "scope": null },
                { "kind": "weekly_all", "percent": 42, "resets_at": "2026-11-18T12:00:00+00:00", "scope": null },
                { "kind": "weekly_scoped", "percent": 0, "resets_at": "2026-11-18T12:00:00+00:00",
                  "scope": { "model": { "id": null, "display_name": "Fable" }, "surface": null } },
                { "kind": "weekly_all", "percent": null },
            ]
        });
        let windows = claude_windows(&body);
        let summary: Vec<_> = windows
            .iter()
            .map(|window| {
                (
                    window.id.as_str(),
                    window.short_label.as_deref(),
                    window.used_percent,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("session", Some("5h"), 93.0),
                ("weekly", Some("wk"), 42.0),
                ("weekly:fable", None, 0.0)
            ]
        );
        assert_eq!(windows[2].label, "Weekly · Fable");
        assert_eq!(
            windows[0].resets_at,
            Some(time::macros::datetime!(2026-11-16 17:40:00.523816 UTC))
        );
    }

    #[test]
    fn claude_answers_without_a_list_use_their_fields() {
        let body = json!({
            "five_hour": { "utilization": "12.5", "resets_at": "2026-11-16T17:40:00+00:00" },
            "seven_day": { "utilization": 40, "resets_at": null },
            "seven_day_opus": null,
        });
        let windows = claude_windows(&body);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].used_percent, 12.5);
        assert_eq!(windows[1].id, "weekly");
    }

    #[test]
    fn codex_limits_are_named_by_their_length() {
        let body = json!({
            "plan_type": "prolite",
            "rate_limit": {
                "primary_window": { "used_percent": 12, "limit_window_seconds": 604_800, "reset_at": 1_795_018_474 },
                "secondary_window": null
            },
            "additional_rate_limits": [
                { "limit_name": "GPT-6", "rate_limit": {
                    "primary_window": { "used_percent": 3, "limit_window_seconds": 18000 } } }
            ],
            "code_review_rate_limit": null
        });
        let windows = codex_windows(&body);
        let summary: Vec<_> = windows
            .iter()
            .map(|window| {
                (
                    window.id.as_str(),
                    window.label.as_str(),
                    window.short_label.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("weekly", "Weekly", Some("wk")),
                ("gpt-6:session", "Session (5 hours) · GPT-6", None)
            ]
        );
        assert_eq!(codex_plan("prolite"), "Pro Lite");
        assert_eq!(codex_plan("plus"), "Plus");
    }

    #[test]
    fn codex_limits_of_other_lengths_and_unknown_ones() {
        let body = json!({ "rate_limit": {
            "primary_window": { "used_percent": 50, "limit_window_seconds": 86400 },
            "secondary_window": { "used_percent": 7 }
        }});
        let windows = codex_windows(&body);
        assert_eq!(windows[0].label, "1 day");
        assert_eq!(windows[1].id, "secondary");
        assert_eq!(windows[1].short_label, None);
    }

    #[test]
    fn jwt_expiry_reads_the_exp_claim() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"exp":1795018474}"#);
        let token = format!("header.{payload}.signature");
        assert_eq!(
            jwt_expiry(&token).map(OffsetDateTime::unix_timestamp),
            Some(1_795_018_474)
        );
        assert_eq!(jwt_expiry("not-a-jwt"), None);
    }
}
