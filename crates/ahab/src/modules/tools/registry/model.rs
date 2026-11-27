//! Tool records: Ahab's own recipes for language servers,
//! formatters, linters and check commands, each saying how to get the tool
//! and how to run it; and which tools serve which language.
//!
//! Records cross releases (bundled, from plugins, in settings, sent to
//! daemons), so every reader tolerates what it does not know: unknown
//! fields are ignored, unknown kinds and sources have a catch-all.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a tool does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolKind {
    LanguageServer,
    Formatter,
    Linter,
    Check,
    /// A kind from a newer Ahab: listed, never run.
    #[serde(other)]
    Unknown,
}

impl ToolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LanguageServer => "languageServer",
            Self::Formatter => "formatter",
            Self::Linter => "linter",
            Self::Check => "check",
            Self::Unknown => "unknown",
        }
    }
}

/// One release file for a platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    /// The file's name in the release (`githubRelease`), or its address
    /// (`download`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The file's SHA-256, in hex. A record without one cannot be installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// The binaries' paths in this file, when they differ from the
    /// record's `bin` (an archive with a folder per platform).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bin: BTreeMap<String, String>,
}

/// Where a tool comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Source {
    Npm {
        package: String,
        version: String,
        /// Other packages installed beside it, pinned (`typescript@5.9.3`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        extra: Vec<String>,
    },
    Pypi {
        package: String,
        version: String,
    },
    Cargo {
        #[serde(rename = "crate")]
        krate: String,
        version: String,
        /// A Git repository instead of crates.io; `version` is then its tag.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        git: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        features: Vec<String>,
    },
    Go {
        module: String,
        version: String,
    },
    GithubRelease {
        repo: String,
        version: String,
        /// The release's tag, when not the version itself (`v1.2.0`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tag: Option<String>,
        /// Per platform (`linux-x64`, `darwin-arm64`), or `any`.
        assets: BTreeMap<String, Asset>,
    },
    /// A file from an address that is not a GitHub release (jdtls).
    Download {
        version: String,
        assets: BTreeMap<String, Asset>,
    },
    /// Never installed by Ahab: found, and explained by the record's `hint`.
    System,
    /// A source from a newer Ahab: not installable here.
    #[serde(other)]
    Unknown,
}

impl Source {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Npm { .. } => "npm",
            Self::Pypi { .. } => "pypi",
            Self::Cargo { .. } => "cargo",
            Self::Go { .. } => "go",
            Self::GithubRelease { .. } => "githubRelease",
            Self::Download { .. } => "download",
            Self::System => "system",
            Self::Unknown => "unknown",
        }
    }

    /// The pinned version, for sources that have one.
    pub fn version(&self) -> Option<&str> {
        match self {
            Self::Npm { version, .. }
            | Self::Pypi { version, .. }
            | Self::Cargo { version, .. }
            | Self::Go { version, .. }
            | Self::GithubRelease { version, .. }
            | Self::Download { version, .. } => Some(version),
            Self::System | Self::Unknown => None,
        }
    }

    /// The runtime installing it needs, beyond what running it needs.
    pub fn installer(&self) -> Option<&'static str> {
        match self {
            Self::Npm { .. } => Some("node"),
            Self::Pypi { .. } => Some("python"),
            Self::Cargo { .. } => Some("rust"),
            Self::Go { .. } => Some("go"),
            _ => None,
        }
    }

    /// The release file for `platform`, or the one for every platform.
    pub fn asset(&self, platform: &str) -> Option<&Asset> {
        match self {
            Self::GithubRelease { assets, .. } | Self::Download { assets, .. } => {
                assets.get(platform).or_else(|| assets.get("any"))
            }
            _ => None,
        }
    }

    /// Whether Ahab can install it on `platform` at all.
    pub fn installable_on(&self, platform: &str) -> bool {
        match self {
            Self::Npm { .. } | Self::Pypi { .. } | Self::Cargo { .. } | Self::Go { .. } => true,
            Self::GithubRelease { .. } | Self::Download { .. } => self
                .asset(platform)
                .is_some_and(|asset| asset.sha256.is_some()),
            Self::System | Self::Unknown => false,
        }
    }
}

/// A runtime a tool needs: `node`, `java`, `python`, `go`, `rust`, and the
/// lowest major version it takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub runtime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<u32>,
}

/// How a linter's or a check's output becomes Diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub format: OutputFormat,
    /// Which stream to read: `stdout` (the default), `stderr` or `both`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<JsonShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<Pattern>,
    /// Report files to read instead of the command's output, after it ends
    /// (globs relative to the root: `build/reports/checkstyle/*.xml`), in
    /// the declared format. Only reports written while it ran count.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputFormat {
    Sarif,
    /// Checkstyle's XML, which PMD, `SpotBugs`, ktlint and detekt write too.
    Checkstyle,
    Json,
    Pattern,
    #[serde(other)]
    Unknown,
}

/// A declared JSON shape: where the problems are and which field is which.
/// Paths are dotted (`message.spans`), `[]` walks an array, `[k=v]` takes
/// the first element whose `k` is `v`, and `../` climbs to the item the
/// current one was found in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonShape {
    /// One JSON document per line (`cargo --message-format=json`).
    #[serde(default)]
    pub lines: bool,
    /// The walk from a document to each problem, such as `[].messages[]`;
    /// the document itself when empty.
    #[serde(default)]
    pub each: String,
    /// Fields an item must have to count (`{"reason": "compiler-message"}`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, Value>,
    pub path: String,
    pub line: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
    /// The tool's severity words (or numbers, as text) to Ahab's:
    /// `error`, `warning`, `information`, `hint`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub severities: BTreeMap<String, String>,
    /// Whether columns count from 1 (the default) or 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_base: Option<u32>,
}

/// A line pattern, like VS Code's problem matchers: a regular expression
/// with the named groups `file`, `line`, `column`, `endLine`,
/// `endColumn`, `severity`, `code` and `message`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pattern {
    pub regexp: String,
    /// The severity when the line names none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub severities: BTreeMap<String, String>,
}

/// How a tool starts and where.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    /// The command; its first word is one of the record's binaries (or a
    /// path). `${path}` is the file (formatters, linters), `${root}` the
    /// root, `${toolDir}` the installed tool's folder, `${cache}` a folder
    /// for the tool's own data per root.
    #[serde(default)]
    pub command: Vec<String>,
    /// The files that mark a root, nearest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub root_markers: Vec<String>,
    /// Prefer the top-most marker whose text contains this
    /// (`[workspace]` in `Cargo.toml`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_most_containing: Option<String>,
    /// The project's own files that configure the tool, shown in its status.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings_files: Vec<String>,
    /// The tool runs only where one of these exists between the file and
    /// the project's folder (biome, eslint, prettier: "whichever the
    /// project configures").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_files: Vec<String>,
    /// A language server's `initializationOptions`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initialization_options: Option<Value>,
    /// What a language server gets for `workspace/configuration`, by
    /// section (`{"eslint": {…}}`), under the settings' own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Value>,
    /// A linter's or a check's output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Output>,
    /// Asks a tool for its version; `--version` when absent, nothing when
    /// empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_args: Option<Vec<String>>,
    /// Extra environment.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// When a check runs, unless the project says: `save`, `agentTurn`,
    /// `request`. Every one when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<Vec<String>>,
    /// The language IDs the server knows the languages by, when not
    /// Ahab's (`tsx` → `typescriptreact`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub language_ids: BTreeMap<String, String>,
}

/// One tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRecord {
    /// Stable, lower-case with dashes; never renamed.
    pub id: String,
    pub name: String,
    pub kind: ToolKind,
    /// Ahab's language IDs it serves.
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Absent for your own tools: found, never installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<Requirement>,
    /// Command names to their path in the installed tool's folder.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bin: BTreeMap<String, String>,
    #[serde(default)]
    pub run: Run,
    /// How to get it without Ahab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// A plugin's record that takes a default's place (`"eslint"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
}

impl ToolRecord {
    /// The command name it runs as: the first word of its command.
    pub fn program(&self) -> Option<&str> {
        self.run.command.first().map(String::as_str)
    }

    /// The binaries to look for: those it names, else its program.
    pub fn binaries(&self) -> Vec<&str> {
        if self.bin.is_empty() {
            self.program().into_iter().collect()
        } else {
            self.bin.keys().map(String::as_str).collect()
        }
    }

    /// Problems with it, as field paths and messages; none when valid.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if !valid_id(&self.id) {
            problems.push("id: use lower-case letters, digits, dots and dashes".to_owned());
        }
        if self.name.trim().is_empty() {
            problems.push("name: give it a name".to_owned());
        }
        if self.kind != ToolKind::Unknown && self.run.command.is_empty() {
            problems.push("run.command: say how it runs".to_owned());
        }
        if matches!(self.kind, ToolKind::Linter | ToolKind::Check) && self.run.output.is_none() {
            problems.push("run.output: a linter or check declares its output".to_owned());
        }
        if let Some(output) = &self.run.output {
            match output.format {
                OutputFormat::Json if output.json.is_none() => {
                    problems.push("run.output.json: declare the JSON shape".to_owned());
                }
                OutputFormat::Pattern => match &output.pattern {
                    None => problems.push("run.output.pattern: give the pattern".to_owned()),
                    Some(pattern) if regex::Regex::new(&pattern.regexp).is_err() => {
                        problems.push("run.output.pattern.regexp: not a valid pattern".to_owned());
                    }
                    Some(_) => {}
                },
                _ => {}
            }
        }
        problems
    }
}

/// Whether `id` is a valid tool or language ID.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
}

/// This computer's platform as records name it (`linux-x64`).
pub fn platform() -> String {
    platform_of(std::env::consts::OS, std::env::consts::ARCH)
}

pub fn platform_of(os: &str, arch: &str) -> String {
    let os = match os {
        "macos" => "darwin",
        other => other,
    };
    let arch = match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    format!("{os}-{arch}")
}

/// Ahab's languages and their tools, in order of preference.
pub type LanguageTools = BTreeMap<String, Vec<String>>;

/// Plugins' records over the defaults: a new ID adds a tool, the same ID or
/// `replaces` takes a default's place (in every language that lists it).
pub fn with_contributed(
    records: &mut Vec<ToolRecord>,
    languages: &mut LanguageTools,
    contributed: Vec<ToolRecord>,
) {
    for record in contributed {
        if let Some(replaced) = record.replaces.clone() {
            records.retain(|existing| existing.id != replaced);
            for tools in languages.values_mut() {
                for tool in tools.iter_mut() {
                    if *tool == replaced {
                        tool.clone_from(&record.id);
                    }
                }
            }
        }
        // A contributed tool serves its languages, after their defaults.
        for language in &record.languages {
            let tools = languages.entry(language.clone()).or_default();
            if !tools.contains(&record.id) {
                tools.push(record.id.clone());
            }
        }
        records.retain(|existing| existing.id != record.id);
        records.push(record);
    }
}

/// LSP's language ID for Ahab's, where they differ.
pub fn lsp_language_id(record: &ToolRecord, language: &str) -> String {
    if let Some(id) = record.run.language_ids.get(language) {
        return id.clone();
    }
    match language {
        "tsx" => "typescriptreact",
        "bash" => "shellscript",
        "c-sharp" => "csharp",
        "markdown.inline" => "markdown",
        other => other,
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_read_what_they_know_and_keep_going() {
        let record: ToolRecord = serde_json::from_value(serde_json::json!({
            "id": "zls", "name": "zls", "kind": "languageServer", "languages": ["zig"],
            "source": { "type": "brew", "formula": "zls" },
            "run": { "command": ["zls"], "rootMarkers": ["build.zig"], "future": 1 },
            "colour": "orange"
        }))
        .unwrap();
        assert_eq!(record.source, Some(Source::Unknown));
        assert_eq!(record.binaries(), ["zls"]);
        assert!(record.problems().is_empty());
        let odd: ToolRecord = serde_json::from_value(serde_json::json!({
            "id": "x", "name": "x", "kind": "telepathy"
        }))
        .unwrap();
        assert_eq!(odd.kind, ToolKind::Unknown);
    }

    #[test]
    fn linters_declare_their_output() {
        let record: ToolRecord = serde_json::from_value(serde_json::json!({
            "id": "Bad Id", "name": "", "kind": "linter",
            "run": { "command": ["lint"] }
        }))
        .unwrap();
        assert_eq!(record.problems().len(), 3);
    }

    #[test]
    fn plugins_add_and_replace_tools() {
        let record = |id: &str, replaces: Option<&str>| ToolRecord {
            id: id.to_owned(),
            name: id.to_owned(),
            kind: ToolKind::LanguageServer,
            languages: vec!["typescript".to_owned()],
            license: None,
            homepage: None,
            description: None,
            source: None,
            requires: vec![],
            bin: BTreeMap::new(),
            run: Run {
                command: vec![id.to_owned()],
                ..Run::default()
            },
            hint: None,
            replaces: replaces.map(str::to_owned),
        };
        let mut records = vec![record("eslint", None), record("tsls", None)];
        let mut languages = LanguageTools::from([(
            "typescript".to_owned(),
            vec!["tsls".to_owned(), "eslint".to_owned()],
        )]);
        with_contributed(
            &mut records,
            &mut languages,
            vec![record("oxlint", Some("eslint")), record("extra", None)],
        );
        assert_eq!(languages["typescript"], ["tsls", "oxlint", "extra"]);
        let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["tsls", "oxlint", "extra"]);
    }

    #[test]
    fn platforms_are_named_as_releases_name_them() {
        assert_eq!(platform_of("linux", "x86_64"), "linux-x64");
        assert_eq!(platform_of("macos", "aarch64"), "darwin-arm64");
    }
}
