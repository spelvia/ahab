//! A plugin as Ahab knows it, and its manifest (`ahab-plugin.json`) with
//! the rules it must follow.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::shared::error::FieldError;

/// The plugin API's version this Ahab offers; a manifest's `ahab` range
/// must include it. 0.1.1 added `contributes.languages`, 0.1.2
/// `contributes.tools` and languages' `tools`, 0.1.3 the daemon
/// entry's diagnostics API and Checkstyle and report files in tool records.
pub const PLUGIN_API: &str = "0.1.4";

/// The hooks a plugin may intercept.
pub const HOOKS: [&str; 2] = ["terminals.create", "notifications.send"];

/// The name of the manifest in a plugin's folder.
pub const MANIFEST: &str = "ahab-plugin.json";

/// Where a plugin was installed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Folder,
    Git,
    /// A folder being worked on (`ahab plugins dev`), followed as it changes.
    Dev,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Git => "git",
            Self::Dev => "dev",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "git" => Self::Git,
            "dev" => Self::Dev,
            _ => Self::Folder,
        }
    }
}

/// Where a plugin's daemon entry runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Placement {
    /// On every machine.
    #[default]
    All,
    /// On the machines the person picks.
    Chosen,
}

/// An entry: a built file, or (for Rust entries) a binary per platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Entry {
    File(String),
    Binaries(BTreeMap<String, String>),
}

impl Entry {
    /// Every path the entry names.
    pub fn paths(&self) -> Vec<&str> {
        match self {
            Self::File(path) => vec![path.as_str()],
            Self::Binaries(paths) => paths.values().map(String::as_str).collect(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entries {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web: Option<Entry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<Entry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon: Option<Entry>,
}

impl Entries {
    pub fn all(&self) -> impl Iterator<Item = (&'static str, &Entry)> {
        [
            ("web", self.web.as_ref()),
            ("server", self.server.as_ref()),
            ("daemon", self.daemon.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, entry)| entry.map(|entry| (name, entry)))
    }
}

/// What a plugin adds by declaring it, without code (experimental).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contributes {
    /// Languages: file types, a Tree-sitter grammar, queries. Kept
    /// as written, since the web app reads them and language tooling adds fields.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<Value>,
    /// Tool records: language servers, formatters, linters and
    /// checks, in the registry's format; read by language tooling.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Value>,
}

/// What `contributes` may hold; anything else is kept, with a warning.
const CONTRIBUTIONS: [&str; 2] = ["languages", "tools"];

/// A contributed language's ID: lowercase, as `rust` or `markdown.inline`.
fn valid_language_id(id: &str) -> bool {
    let mut chars = id.chars();
    (1..=40).contains(&id.len())
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._+#-".contains(c))
}

impl Contributes {
    pub fn is_empty(&self) -> bool {
        self.languages.is_empty() && self.tools.is_empty()
    }

    /// The files the languages name (grammars and queries), as written.
    pub fn files(&self) -> Vec<String> {
        let mut files = Vec::new();
        for language in &self.languages {
            if let Some(path) = language["grammar"]["wasm"].as_str() {
                files.push(path.to_owned());
            }
            if let Some(queries) = language["queries"].as_object() {
                files.extend(
                    queries
                        .values()
                        .filter_map(Value::as_str)
                        .map(str::to_owned),
                );
            }
        }
        files.sort();
        files.dedup();
        files
    }

    /// The rules each language declaration must follow.
    fn problems(&self) -> Vec<FieldError> {
        let mut problems = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let fail = |problems: &mut Vec<FieldError>, index: usize, message: &str| {
            problems.push(FieldError::new(
                "contributes.languages",
                format!("language {}: {message}", index + 1),
            ));
        };
        for (index, tool) in self.tools.iter().enumerate() {
            let id = tool["id"].as_str().unwrap_or_default();
            let named = !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.');
            if !named || tool["kind"].as_str().is_none() || !tool["run"]["command"].is_array() {
                problems.push(FieldError::new(
                    "contributes.tools",
                    format!(
                        "tool {}: needs an id (lowercase letters, digits, dots and dashes), a kind and run.command",
                        index + 1
                    ),
                ));
            }
        }
        for (index, language) in self.languages.iter().enumerate() {
            let Some(fields) = language.as_object() else {
                fail(&mut problems, index, "must be an object");
                continue;
            };
            match fields.get("id").and_then(Value::as_str) {
                Some(id) if valid_language_id(id) => {
                    if !seen.insert(id.to_owned()) {
                        fail(&mut problems, index, &format!("{id} is declared twice"));
                    }
                }
                _ => fail(
                    &mut problems,
                    index,
                    "id must be lowercase letters, digits and . _ + # -, starting with a letter, at most 40 long",
                ),
            }
            if fields
                .get("name")
                .and_then(Value::as_str)
                .is_none_or(|name| name.trim().is_empty())
            {
                fail(&mut problems, index, "name must not be empty");
            }
            if let Some(files) = fields.get("files") {
                let lists_of_text = files.as_object().is_some_and(|files| {
                    files.values().all(|list| {
                        list.as_array()
                            .is_some_and(|items| items.iter().all(Value::is_string))
                    })
                });
                if !lists_of_text {
                    fail(
                        &mut problems,
                        index,
                        "files must be an object of lists of text (extensions, names, shebangs)",
                    );
                }
            }
            match fields.get("grammar") {
                None => {}
                Some(grammar) => match grammar["wasm"].as_str() {
                    Some(path) if inside(path) => {}
                    _ => fail(
                        &mut problems,
                        index,
                        "grammar.wasm must be a path inside the plugin's folder",
                    ),
                },
            }
            if let Some(queries) = fields.get("queries") {
                let paths = queries.as_object().is_some_and(|queries| {
                    queries
                        .values()
                        .all(|path| path.as_str().is_some_and(inside))
                });
                if !paths {
                    fail(
                        &mut problems,
                        index,
                        "queries must be paths inside the plugin's folder",
                    );
                }
            }
        }
        problems
    }
}

/// `ahab-plugin.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The plugin API versions it works with (a semver range).
    pub ahab: String,
    /// Its built entries; none for a plugin that only contributes.
    #[serde(default)]
    pub entries: Entries,
    #[serde(default)]
    pub depends_on: BTreeMap<String, String>,
    #[serde(default)]
    pub secrets: Vec<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub machines: Placement,
    /// Its settings' JSON Schema, when it has settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Value>,
    /// What it adds by declaring it.
    #[serde(default, skip_serializing_if = "Contributes::is_empty")]
    pub contributes: Contributes,
    /// The notifications it sends: only these, so each can be
    /// turned off in Settings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notifications: Vec<DeclaredKind>,
    /// Where it can deliver notifications: ntfy, Slack, email.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub destinations: Vec<DeclaredDestination>,
}

/// A kind of notification a plugin sends; its full kind is
/// `<plugin>.<kind>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredKind {
    /// `pipeline.failed`: lowercase letters, digits, dashes and dots.
    pub kind: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A place a plugin delivers notifications to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredDestination {
    /// `ntfy`: lowercase letters, digits and dashes.
    pub id: String,
    pub name: String,
}

/// Whether `kind` names a plugin's notification: lowercase letters, digits
/// and dashes, in parts joined by dots, at most 60 long.
fn valid_kind(kind: &str) -> bool {
    kind.len() <= 60
        && kind.split('.').all(|part| {
            part.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

/// The fields a manifest may have; others are kept, with a warning.
const KNOWN: [&str; 14] = [
    "id",
    "name",
    "version",
    "description",
    "ahab",
    "entries",
    "dependsOn",
    "secrets",
    "hooks",
    "machines",
    "settings",
    "contributes",
    "notifications",
    "destinations",
];

/// IDs a plugin cannot take: `commands.call` reads `terminals.…` as Ahab's
/// own commands for plugins, and `ahab` is Ahab.
const RESERVED_IDS: [&str; 2] = ["ahab", "terminals"];

/// Whether `id` is a plugin ID: lowercase letters, digits and dashes,
/// starting with a letter, 2 to 40 long, and not reserved.
fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    !RESERVED_IDS.contains(&id)
        && (2..=40).contains(&id.len())
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.ends_with('-')
}

/// What `manifest`'s `dependsOn` lacks among `plugins`: each a sentence,
/// such as "docker is not installed".
pub fn unmet(manifest: &Manifest, plugins: &[Plugin]) -> Vec<String> {
    manifest
        .depends_on
        .iter()
        .filter_map(|(id, range)| {
            let Some(found) = plugins.iter().find(|plugin| &plugin.id == id) else {
                return Some(format!("{id} is not installed"));
            };
            let fits = semver::Version::parse(&found.manifest.version)
                .ok()
                .zip(version_range(range))
                .is_some_and(|(version, range)| range.matches(&version));
            if !fits {
                return Some(format!(
                    "{id} {} is installed, and it needs {range}",
                    found.manifest.version
                ));
            }
            (!found.enabled()).then(|| format!("{id} is not enabled"))
        })
        .collect()
}

/// The enabled plugins among `plugins` whose `dependsOn` names `id`.
pub fn needed_by<'a>(id: &str, plugins: &'a [Plugin]) -> Vec<&'a str> {
    plugins
        .iter()
        .filter(|plugin| plugin.enabled() && plugin.manifest.depends_on.contains_key(id))
        .map(|plugin| plugin.id.as_str())
        .collect()
}

/// A version range as npm writes it (`>=0.1.0 <0.2.0`, `>= 1.2`) or as
/// Rust does (`>=0.1.0, <0.2.0`).
pub fn version_range(text: &str) -> Option<semver::VersionReq> {
    let mut parts: Vec<String> = Vec::new();
    let mut operator = String::new();
    for token in text.split(|c: char| c.is_whitespace() || c == ',') {
        if token.is_empty() {
            continue;
        }
        if token.chars().all(|c| "<>=^~".contains(c)) {
            operator.push_str(token);
        } else {
            parts.push(format!("{operator}{token}"));
            operator.clear();
        }
    }
    if !operator.is_empty() || parts.is_empty() {
        return None;
    }
    semver::VersionReq::parse(&parts.join(", ")).ok()
}

/// A build's digest: SHA-256 over its files' paths (with `/`) and
/// contents, in path order, as hex. The server makes it when installing;
/// a daemon checks a build it receives against it.
pub fn digest_of(files: &[(String, Vec<u8>)]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hash = Sha256::new();
    for (path, bytes) in sorted {
        hash.update(path.as_bytes());
        hash.update([0]);
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    hash.finalize().iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// Whether `path` stays inside the plugin's folder.
pub fn inside(path: &str) -> bool {
    let path = std::path::Path::new(path);
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

impl Manifest {
    /// Reads and checks a manifest: the manifest and its warnings, or every
    /// rule it breaks.
    pub fn parse(text: &str) -> Result<(Self, Vec<String>), Vec<FieldError>> {
        let value: Value = serde_json::from_str(text)
            .map_err(|error| vec![FieldError::new("manifest", format!("not JSON: {error}"))])?;
        let mut warnings = Vec::new();
        if let Value::Object(fields) = &value {
            for key in fields.keys() {
                if !KNOWN.contains(&key.as_str()) {
                    warnings.push(format!("unknown field {key}, ignored"));
                }
            }
            if let Some(Value::Object(contributes)) = fields.get("contributes") {
                for key in contributes.keys() {
                    if !CONTRIBUTIONS.contains(&key.as_str()) {
                        warnings.push(format!("unknown contribution {key}, ignored"));
                    }
                }
            }
        }
        let manifest: Self = serde_json::from_value(value)
            .map_err(|error| vec![FieldError::new("manifest", error.to_string())])?;
        let mut problems = Vec::new();
        if !valid_id(&manifest.id) {
            problems.push(FieldError::new(
                "id",
                "lowercase letters, digits and dashes, starting with a letter, 2 to 40 long; not ahab or terminals",
            ));
        }
        if manifest.name.trim().is_empty() {
            problems.push(FieldError::new("name", "must not be empty"));
        }
        if semver::Version::parse(&manifest.version).is_err() {
            problems.push(FieldError::new(
                "version",
                "must be a version such as 0.1.0",
            ));
        }
        match version_range(&manifest.ahab) {
            Some(range) => {
                let ours =
                    semver::Version::parse(PLUGIN_API).unwrap_or(semver::Version::new(0, 0, 0));
                if !range.matches(&ours) {
                    problems.push(FieldError::new(
                        "ahab",
                        format!(
                            "{} does not include this Ahab's plugin API, {PLUGIN_API}",
                            manifest.ahab
                        ),
                    ));
                }
            }
            None => problems.push(FieldError::new(
                "ahab",
                "must be a version range such as >=0.1.0 <0.2.0",
            )),
        }
        if manifest.entries.all().next().is_none() && manifest.contributes.is_empty() {
            problems.push(FieldError::new(
                "entries",
                "names no entry (web, server or daemon), and the plugin contributes nothing",
            ));
        }
        problems.extend(manifest.contributes.problems());
        for (name, entry) in manifest.entries.all() {
            if entry.paths().iter().any(|path| !inside(path)) {
                problems.push(FieldError::new(
                    "entries",
                    format!("{name}: must be a path inside the plugin's folder"),
                ));
            }
        }
        for hook in &manifest.hooks {
            if !HOOKS.contains(&hook.as_str()) {
                problems.push(FieldError::new(
                    "hooks",
                    format!("{hook} is not a hook Ahab offers ({})", HOOKS.join(", ")),
                ));
            }
        }
        let mut kinds = std::collections::BTreeSet::new();
        for declared in &manifest.notifications {
            if !valid_kind(&declared.kind) {
                problems.push(FieldError::new(
                    "notifications",
                    format!(
                        "{}: lowercase letters, digits and dashes in parts joined by dots",
                        declared.kind
                    ),
                ));
            } else if !kinds.insert(declared.kind.as_str()) {
                problems.push(FieldError::new(
                    "notifications",
                    format!("{} is declared twice", declared.kind),
                ));
            }
            if declared.title.trim().is_empty() {
                problems.push(FieldError::new(
                    "notifications",
                    format!("{}: needs a title", declared.kind),
                ));
            }
        }
        let mut destinations = std::collections::BTreeSet::new();
        for destination in &manifest.destinations {
            if !valid_id(&destination.id) || !destinations.insert(destination.id.as_str()) {
                problems.push(FieldError::new(
                    "destinations",
                    format!(
                        "{}: lowercase letters, digits and dashes, once each",
                        destination.id
                    ),
                ));
            }
        }
        for (plugin, range) in &manifest.depends_on {
            if version_range(range).is_none() {
                problems.push(FieldError::new(
                    "dependsOn",
                    format!("{plugin}: must be a version range"),
                ));
            }
        }
        if !manifest.secrets.is_empty() {
            warnings.push("secrets are not supported yet and are not asked for".to_owned());
        }
        if manifest.entries.server.is_some() {
            warnings.push("server entries are not supported yet; this one does not run".to_owned());
        }
        if problems.is_empty() {
            Ok((manifest, warnings))
        } else {
            Err(problems)
        }
    }
}

/// An installed plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub source: Source,
    /// The folder or Git URL it was installed from.
    pub location: String,
    /// SHA-256 of its installed files: which build this is.
    pub digest: String,
    pub manifest: Manifest,
    /// When it was enabled; `None` while off. Also the interceptors' order.
    pub enabled_at: Option<OffsetDateTime>,
    /// Its place among interceptors, as the person ordered them.
    pub position: i32,
    /// Where its daemon entry runs when its manifest says `chosen`.
    pub machines: Vec<Uuid>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl Plugin {
    pub fn enabled(&self) -> bool {
        self.enabled_at.is_some()
    }

    /// Whether its daemon entry runs on `machine`.
    pub fn runs_on(&self, machine: Uuid) -> bool {
        self.enabled()
            && self.manifest.entries.daemon.is_some()
            && match self.manifest.machines {
                Placement::All => true,
                Placement::Chosen => self.machines.contains(&machine),
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
        "id": "docker", "name": "Docker", "version": "0.1.0",
        "ahab": ">=0.1.0 <0.2.0",
        "entries": { "web": "dist/web.js", "daemon": "dist/daemon.js" },
        "hooks": ["terminals.create"]
    }"#;

    fn problems(text: &str) -> Vec<String> {
        Manifest::parse(text)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|problem| problem.field.into_owned())
            .collect()
    }

    #[test]
    fn a_good_manifest_is_read() {
        let (manifest, warnings) = Manifest::parse(GOOD).unwrap();
        assert_eq!(manifest.id, "docker");
        assert_eq!(manifest.machines, Placement::All);
        assert!(warnings.is_empty());
    }

    #[test]
    fn notification_kinds_and_destinations_are_checked() {
        let with = |notifications: &str, destinations: &str| {
            let mut json: Value = serde_json::from_str(GOOD).unwrap();
            json["notifications"] = serde_json::from_str(notifications).unwrap();
            json["destinations"] = serde_json::from_str(destinations).unwrap();
            json.to_string()
        };
        let (manifest, warnings) = Manifest::parse(&with(
            r#"[{ "kind": "pipeline.failed", "title": "Pipeline failed" }]"#,
            r#"[{ "id": "ntfy", "name": "ntfy" }]"#,
        ))
        .unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(manifest.notifications[0].kind, "pipeline.failed");
        assert_eq!(manifest.destinations[0].id, "ntfy");
        assert_eq!(
            problems(&with(
                r#"[{ "kind": "Pipeline", "title": "x" }, { "kind": "a", "title": " " }, { "kind": "a", "title": "y" }]"#,
                r#"[{ "id": "NT", "name": "x" }]"#,
            )),
            [
                "notifications",
                "notifications",
                "notifications",
                "destinations"
            ]
        );
    }

    #[test]
    fn every_rule_is_checked() {
        let with = |key: &str, value: &str| {
            let mut json: Value = serde_json::from_str(GOOD).unwrap();
            json[key] = serde_json::from_str(value).unwrap();
            json.to_string()
        };
        assert_eq!(problems(&with("id", "\"Docker\"")), ["id"]);
        assert_eq!(problems(&with("version", "\"one\"")), ["version"]);
        assert_eq!(problems(&with("ahab", "\">=1.0.0\"")), ["ahab"]);
        assert_eq!(problems(&with("entries", "{}")), ["entries"]);
        assert_eq!(
            problems(&with("entries", "{\"web\": \"../web.js\"}")),
            ["entries"]
        );
        assert_eq!(problems(&with("hooks", "[\"files.write\"]")), ["hooks"]);
        assert_eq!(problems("{"), ["manifest"]);
        let (_, warnings) = Manifest::parse(&with("colour", "\"blue\"")).unwrap();
        assert_eq!(warnings, ["unknown field colour, ignored"]);
    }

    #[test]
    fn languages_are_contributed_by_declaring_them() {
        let manifest = r#"{
            "id": "zig", "name": "Zig", "version": "0.1.0", "ahab": ">=0.1.1 <0.2.0",
            "contributes": { "languages": [{
                "id": "zig", "name": "Zig",
                "files": { "extensions": ["zig"] },
                "grammar": { "wasm": "grammars/zig.wasm" },
                "queries": { "highlights": "queries/highlights.scm", "tags": "queries/tags.scm" },
                "servers": ["zls"]
            }], "themes": [] }
        }"#;
        let (manifest, warnings) = Manifest::parse(manifest).unwrap();
        assert_eq!(warnings, ["unknown contribution themes, ignored"]);
        assert_eq!(
            manifest.contributes.files(),
            [
                "grammars/zig.wasm",
                "queries/highlights.scm",
                "queries/tags.scm"
            ]
        );
        // Kept as written, for the web app and language tooling.
        let json = serde_json::to_value(&manifest).unwrap();
        assert_eq!(json["contributes"]["languages"][0]["servers"][0], "zls");
        assert_eq!(json["entries"], serde_json::json!({}));
    }

    #[test]
    fn contributed_languages_follow_the_rules() {
        let with = |languages: &str| {
            format!(
                r#"{{"id": "zig", "name": "Zig", "version": "0.1.0", "ahab": ">=0.1.1",
                    "contributes": {{"languages": {languages}}}}}"#
            )
        };
        for bad in [
            r#"[{"id": "Zig", "name": "Zig"}]"#,
            r#"[{"id": "zig", "name": ""}]"#,
            r#"[{"id": "zig", "name": "Zig", "grammar": {"wasm": "../zig.wasm"}}]"#,
            r#"[{"id": "zig", "name": "Zig", "queries": {"highlights": "/etc/x.scm"}}]"#,
            r#"[{"id": "zig", "name": "Zig", "files": {"extensions": "zig"}}]"#,
            r#"[{"id": "zig", "name": "Zig"}, {"id": "zig", "name": "Zig 2"}]"#,
            r#"["zig"]"#,
        ] {
            assert_eq!(problems(&with(bad)), ["contributes.languages"], "{bad}");
        }
        // Without entries, a plugin must contribute something.
        assert!(problems(&with("[]")).contains(&"entries".to_owned()));
    }

    #[test]
    fn version_ranges_are_read_as_npm_and_rust_write_them() {
        let ours = semver::Version::parse(PLUGIN_API).unwrap();
        for text in [
            ">=0.1.0 <0.2.0",
            ">=0.1.0, <0.2.0",
            ">= 0.1.0 < 0.2.0",
            "^0.1",
            "0.1.0",
        ] {
            assert!(
                version_range(text).is_some_and(|range| range.matches(&ours)),
                "{text}"
            );
        }
        assert!(version_range(">=").is_none());
        assert!(version_range("").is_none());
        assert!(version_range("one").is_none());
    }

    #[test]
    fn placement_decides_where_the_daemon_entry_runs() {
        let (manifest, _) = Manifest::parse(GOOD).unwrap();
        let machine = Uuid::now_v7();
        let mut plugin = Plugin {
            id: "docker".into(),
            name: "Docker".into(),
            version: "0.1.0".into(),
            description: None,
            source: Source::Folder,
            location: "/p".into(),
            digest: "d".into(),
            manifest,
            enabled_at: None,
            position: 0,
            machines: vec![],
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
        };
        assert!(!plugin.runs_on(machine), "not while disabled");
        plugin.enabled_at = Some(OffsetDateTime::now_utc());
        assert!(plugin.runs_on(machine));
        plugin.manifest.machines = Placement::Chosen;
        assert!(!plugin.runs_on(machine));
        plugin.machines.push(machine);
        assert!(plugin.runs_on(machine));
    }
}
