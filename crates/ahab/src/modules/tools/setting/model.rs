//! Tool settings: which tool, where, and how it starts; never how
//! it behaves (that is the project's own files). The same shape serves
//! every layer: a machine, a project in Ahab, and a repository's
//! `.ahab/tools.json`. Later layers win field by field.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::modules::tools::registry::model::{ToolRecord, valid_id};
use crate::shared::error::FieldError;

/// The format of stored settings (`tool_settings.settings_format`).
pub const SETTINGS_FORMAT: i32 = 1;

/// The most language servers a machine runs at once, by default; the least
/// recently used stops first (open question 1).
pub const DEFAULT_MAX_SERVERS: u32 = 8;

/// The most check commands a machine runs at once, by default; the rest
/// wait.
pub const DEFAULT_MAX_CHECKS: u32 = 2;

/// A repository counts as active this many days after a file in it was
/// opened or changed by an agent, by default.
pub const DEFAULT_ACTIVE_DAYS: u32 = 14;

/// When language servers start, per project.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ServerStart {
    /// When a file of their language opens (the default).
    #[default]
    Open,
    /// Only for files in pinned repositories.
    Pinned,
    /// Never.
    Off,
    /// A choice from a newer release: as `open`.
    #[serde(other)]
    Unknown,
}

/// When a check runs.
pub const TRIGGERS: [&str; 3] = ["save", "agentTurn", "request"];

/// One language's choices.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LanguageChoice {
    /// Its tools, in order of preference, instead of the defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    /// No tools for this language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_on_save: Option<bool>,
    /// The tool that formats, when not the first that can.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatter: Option<String>,
}

/// One tool's choices.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolChoice {
    /// Not run here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off: Option<bool>,
    /// Where it is, instead of looking for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Extra environment (`JAVA_HOME`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// A language server's settings, over the record's
    /// (`workspace/configuration`, `initializationOptions` under `init`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub settings: Option<Value>,
    /// When a check runs: `save`, `agentTurn`, `request`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<Vec<String>>,
}

/// The project's environment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentChoice {
    /// Use the flake's dev shell (`nix develop`); off by default, as it can
    /// take long to start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nix: Option<bool>,
    /// Use an allowed `.envrc` (on by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direnv: Option<bool>,
    /// Use mise or asdf's files (on by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mise: Option<bool>,
    /// Extra variables for every tool.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// One layer of settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolSettings {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub languages: BTreeMap<String, LanguageChoice>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools: BTreeMap<String, ToolChoice>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub environment: EnvironmentChoice,
    /// Your own tools, in the registry's format, without a source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schema(value_type = Vec<Object>)]
    pub records: Vec<ToolRecord>,
    /// A machine's: how many language servers run at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_servers: Option<u32>,
    /// A machine's: keep the LSP traffic in the servers' logs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_traffic: Option<bool>,
    /// A machine's: how many check commands run at once; the rest wait.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_checks: Option<u32>,
    /// A machine's: the memory its language servers may use together, in
    /// MiB; half the machine's memory when absent. Starting a server that
    /// would pass it stops the least recently used first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_budget: Option<u64>,
    /// A project's: when language servers start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub servers: Option<ServerStart>,
    /// A project's: how many days a repository stays active after a file in
    /// it was opened or changed by an agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_days: Option<u32>,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

fn pick<T: Clone>(base: &mut Option<T>, over: Option<&T>) {
    if let Some(over) = over {
        *base = Some(over.clone());
    }
}

impl ToolSettings {
    /// `over` on top of this: a field it sets wins, maps merge key by key,
    /// records with the same ID replace.
    pub fn layer(&mut self, over: &Self) {
        for (id, choice) in &over.languages {
            let base = self.languages.entry(id.clone()).or_default();
            pick(&mut base.tools, choice.tools.as_ref());
            pick(&mut base.off, choice.off.as_ref());
            pick(&mut base.format_on_save, choice.format_on_save.as_ref());
            pick(&mut base.formatter, choice.formatter.as_ref());
        }
        for (id, choice) in &over.tools {
            let base = self.tools.entry(id.clone()).or_default();
            pick(&mut base.off, choice.off.as_ref());
            pick(&mut base.path, choice.path.as_ref());
            pick(&mut base.on, choice.on.as_ref());
            base.env.extend(choice.env.clone());
            if let Some(settings) = &choice.settings {
                let mut merged = base.settings.take().unwrap_or(Value::Null);
                merge_json(&mut merged, settings);
                base.settings = Some(merged);
            }
        }
        pick(&mut self.environment.nix, over.environment.nix.as_ref());
        pick(
            &mut self.environment.direnv,
            over.environment.direnv.as_ref(),
        );
        pick(&mut self.environment.mise, over.environment.mise.as_ref());
        self.environment.env.extend(over.environment.env.clone());
        for record in &over.records {
            self.records.retain(|existing| existing.id != record.id);
            self.records.push(record.clone());
        }
        pick(&mut self.max_servers, over.max_servers.as_ref());
        pick(&mut self.log_traffic, over.log_traffic.as_ref());
        pick(&mut self.max_checks, over.max_checks.as_ref());
        pick(&mut self.memory_budget, over.memory_budget.as_ref());
        pick(&mut self.servers, over.servers.as_ref());
        pick(&mut self.active_days, over.active_days.as_ref());
    }

    /// The layers merged, first to last.
    pub fn merged<'a>(layers: impl IntoIterator<Item = &'a Self>) -> Self {
        let mut merged = Self::default();
        for layer in layers {
            merged.layer(layer);
        }
        merged
    }

    /// What is wrong with it, by field.
    pub fn problems(&self) -> Vec<FieldError> {
        let mut problems = Vec::new();
        for (id, choice) in &self.languages {
            if !valid_id(id) {
                problems.push(FieldError::new(
                    format!("languages.{id}"),
                    "Not a language ID.",
                ));
            }
            for tool in choice.tools.iter().flatten() {
                if !valid_id(tool) {
                    problems.push(FieldError::new(
                        format!("languages.{id}.tools"),
                        format!("{tool} is not a tool ID."),
                    ));
                }
            }
        }
        for (id, choice) in &self.tools {
            if !valid_id(id) {
                problems.push(FieldError::new(format!("tools.{id}"), "Not a tool ID."));
            }
            for trigger in choice.on.iter().flatten() {
                if !TRIGGERS.contains(&trigger.as_str()) {
                    problems.push(FieldError::new(
                        format!("tools.{id}.on"),
                        format!("Use {}.", TRIGGERS.join(", ")),
                    ));
                }
            }
            if choice
                .path
                .as_ref()
                .is_some_and(|path| path.trim().is_empty())
            {
                problems.push(FieldError::new(
                    format!("tools.{id}.path"),
                    "Give a path, or leave it out.",
                ));
            }
        }
        for (index, record) in self.records.iter().enumerate() {
            for problem in record.problems() {
                problems.push(FieldError::new(format!("records.{index}"), problem));
            }
        }
        if self.max_servers.is_some_and(|max| !(1..=64).contains(&max)) {
            problems.push(FieldError::new(
                "maxServers",
                "Use a whole number from 1 to 64.",
            ));
        }
        if self.max_checks.is_some_and(|max| !(1..=16).contains(&max)) {
            problems.push(FieldError::new(
                "maxChecks",
                "Use a whole number from 1 to 16.",
            ));
        }
        if self.memory_budget.is_some_and(|mib| mib < 256) {
            problems.push(FieldError::new(
                "memoryBudget",
                "Give at least 256 MiB, or leave it out for half the machine's memory.",
            ));
        }
        if self
            .active_days
            .is_some_and(|days| !(1..=365).contains(&days))
        {
            problems.push(FieldError::new(
                "activeDays",
                "Use a whole number of days from 1 to 365.",
            ));
        }
        problems
    }

    pub fn max_servers(&self) -> u32 {
        self.max_servers.unwrap_or(DEFAULT_MAX_SERVERS)
    }

    pub fn max_checks(&self) -> u32 {
        self.max_checks.unwrap_or(DEFAULT_MAX_CHECKS)
    }

    pub fn active_days(&self) -> u32 {
        self.active_days.unwrap_or(DEFAULT_ACTIVE_DAYS)
    }

    /// When language servers start.
    pub fn servers(&self) -> ServerStart {
        match self.servers {
            Some(ServerStart::Pinned) => ServerStart::Pinned,
            Some(ServerStart::Off) => ServerStart::Off,
            _ => ServerStart::Open,
        }
    }

    /// Whether `tool` is turned off.
    pub fn is_off(&self, tool: &str) -> bool {
        self.tools.get(tool).and_then(|choice| choice.off) == Some(true)
    }
}

/// `over` merged into `base`: objects key by key, anything else replaced.
pub fn merge_json(base: &mut Value, over: &Value) {
    match (base, over) {
        (Value::Object(base), Value::Object(over)) => {
            for (key, value) in over {
                merge_json(base.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (base, over) => *base = over.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn settings(value: Value) -> ToolSettings {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn later_layers_win_field_by_field() {
        let machine = settings(json!({
            "tools": { "jdtls": { "path": "/opt/jdtls", "env": { "JAVA_HOME": "/jdk21" } } },
            "maxServers": 4
        }));
        let repository = settings(json!({
            "languages": { "typescript": { "tools": ["biome", "typescript-language-server"], "formatOnSave": true } },
            "tools": { "biome": { "settings": { "a": { "b": 1, "c": 2 } } } }
        }));
        let project = settings(json!({
            "languages": { "typescript": { "formatOnSave": false } },
            "tools": { "biome": { "settings": { "a": { "c": 3 } } }, "jdtls": { "env": { "X": "1" } } },
            "environment": { "nix": true }
        }));
        let merged = ToolSettings::merged([&machine, &repository, &project]);
        let typescript = &merged.languages["typescript"];
        assert_eq!(
            typescript.tools.as_deref(),
            Some(&["biome".to_owned(), "typescript-language-server".to_owned()][..])
        );
        assert_eq!(typescript.format_on_save, Some(false));
        assert_eq!(
            merged.tools["biome"].settings,
            Some(json!({ "a": { "b": 1, "c": 3 } }))
        );
        assert_eq!(merged.tools["jdtls"].path.as_deref(), Some("/opt/jdtls"));
        assert_eq!(merged.tools["jdtls"].env.len(), 2);
        assert_eq!(merged.environment.nix, Some(true));
        assert_eq!(merged.max_servers(), 4);
    }

    #[test]
    fn problems_name_their_fields() {
        let bad = settings(json!({
            "tools": { "cargo-clippy": { "on": ["sometimes"] } },
            "records": [{ "id": "mine", "name": "", "kind": "languageServer" }],
            "maxServers": 0
        }));
        let fields: Vec<String> = bad
            .problems()
            .into_iter()
            .map(|p| p.field.into_owned())
            .collect();
        assert_eq!(
            fields,
            [
                "tools.cargo-clippy.on",
                "records.0",
                "records.0",
                "maxServers"
            ]
        );
    }
}
