//! One form for every result: LSP's Diagnostic. Language servers
//! give it as it is; command-line linters and check commands go through
//! adapters, in order of preference: SARIF, Checkstyle XML, a declared JSON
//! shape, a line pattern. And the difference an agent's turn made.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::modules::tools::registry::model::{JsonShape, Output, OutputFormat, Pattern};

/// LSP's severities: 1 error, 2 warning, 3 information, 4 hint.
pub const ERROR: u8 = 1;
pub const WARNING: u8 = 2;
pub const INFORMATION: u8 = 3;
pub const HINT: u8 = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// One problem at a place: LSP's shape, as language servers send it.
/// Fields Ahab does not use are kept (`data`, for a server's fixes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub range: Range,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<u8>,
    /// A rule's name or number (`no-unused-vars`, `E0308`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_information: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Diagnostic {
    pub fn severity(&self) -> u8 {
        self.severity
            .filter(|s| (ERROR..=HINT).contains(s))
            .unwrap_or(ERROR)
    }

    /// Its code as text.
    pub fn code_text(&self) -> Option<String> {
        match &self.code {
            Some(Value::String(code)) => Some(code.clone()),
            Some(Value::Number(number)) => Some(number.to_string()),
            _ => None,
        }
    }

    /// What identifies it across runs, regardless of where it moved:
    /// its severity, code and message.
    pub fn identity(&self) -> (u8, Option<String>, String) {
        (self.severity(), self.code_text(), self.message.clone())
    }
}

/// Ahab's name for an LSP severity.
pub fn severity_name(severity: u8) -> &'static str {
    match severity {
        WARNING => "warning",
        INFORMATION => "information",
        HINT => "hint",
        _ => "error",
    }
}

/// A severity word (`error`, `warn`, `note`, `info`, `style`) as LSP's.
pub fn severity_of(word: &str) -> Option<u8> {
    match word.to_ascii_lowercase().as_str() {
        "error" | "fatal" | "err" | "e" | "failure" => Some(ERROR),
        "warning" | "warn" | "w" => Some(WARNING),
        "information" | "info" | "note" | "i" => Some(INFORMATION),
        "hint" | "help" | "style" | "none" => Some(HINT),
        _ => None,
    }
}

/// Checkstyle's XML (`<checkstyle><file name><error line column severity
/// message source/>`), as Checkstyle, PMD, `SpotBugs`, ktlint and detekt
/// write it. Lines and columns count from 1 (a missing column is the
/// line's start); `ignore` severities are left out; the code is the
/// rule's short name (`WhitespaceAround` for
/// `com.puppycrawl…WhitespaceAroundCheck`).
pub fn from_checkstyle(text: &str, root: &Path) -> Result<Vec<(PathBuf, Diagnostic)>, String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(text);
    let mut found = Vec::new();
    let mut file: Option<PathBuf> = None;
    let mut seen_root = false;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| format!("not Checkstyle XML: {error}"))?;
        let (element, closes) = match &event {
            Event::Start(element) => (element, false),
            Event::Empty(element) => (element, true),
            Event::End(element) => {
                if element.local_name().as_ref() == "file" {
                    file = None;
                }
                continue;
            }
            Event::Eof => break,
            _ => continue,
        };
        let attributes: BTreeMap<String, String> = element
            .attributes()
            .flatten()
            .filter_map(|attribute| {
                let value = attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .ok()?;
                Some((
                    attribute.key.local_name().as_ref().to_owned(),
                    value.into_owned(),
                ))
            })
            .collect();
        match element.local_name().as_ref() {
            "checkstyle" => seen_root = true,
            "file" => {
                file = attributes.get("name").map(|name| resolve(name, root));
                if closes {
                    file = None;
                }
            }
            "error" => {
                let Some(path) = &file else { continue };
                let severity = attributes.get("severity").map_or("error", String::as_str);
                if severity.eq_ignore_ascii_case("ignore") {
                    continue;
                }
                let number = |key: &str| {
                    attributes
                        .get(key)
                        .and_then(|v| v.trim().parse::<u32>().ok())
                };
                let line = number("line").unwrap_or(1).max(1) - 1;
                let column = number("column").unwrap_or(1).max(1) - 1;
                let code = attributes.get("source").map(|source| {
                    let short = if source.contains(':') || source.contains(' ') {
                        source.as_str()
                    } else {
                        source.rsplit('.').next().unwrap_or(source)
                    };
                    let short = short
                        .strip_suffix("Check")
                        .filter(|s| !s.is_empty())
                        .unwrap_or(short);
                    Value::String(short.to_owned())
                });
                found.push((
                    path.clone(),
                    Diagnostic {
                        range: Range {
                            start: Position {
                                line,
                                character: column,
                            },
                            end: Position {
                                line,
                                character: column,
                            },
                        },
                        severity: Some(severity_of(severity).unwrap_or(ERROR)),
                        code,
                        source: None,
                        message: attributes.get("message").cloned().unwrap_or_default(),
                        related_information: None,
                        tags: None,
                        data: None,
                    },
                ));
            }
            _ => {}
        }
    }
    if !seen_root {
        return Err("not Checkstyle XML: no <checkstyle> element".to_owned());
    }
    Ok(found)
}

/// Diagnostics by file.
pub type ByFile = BTreeMap<PathBuf, Vec<Diagnostic>>;

/// A tool's output read with its declared format: absolute paths (relative
/// ones are taken from `root`) to their diagnostics.
pub fn read_output(
    output: &Output,
    text: &str,
    root: &Path,
    source: &str,
) -> Result<ByFile, String> {
    let found = match output.format {
        OutputFormat::Sarif => from_sarif(text, root)?,
        OutputFormat::Checkstyle => from_checkstyle(text, root)?,
        OutputFormat::Json => from_json(
            text,
            output
                .json
                .as_ref()
                .ok_or("the record declares no JSON shape")?,
            root,
        )?,
        OutputFormat::Pattern => from_pattern(
            text,
            output
                .pattern
                .as_ref()
                .ok_or("the record declares no pattern")?,
            root,
        )?,
        OutputFormat::Unknown => return Err("an output format this Ahab does not know".to_owned()),
    };
    let mut by_file = ByFile::new();
    for (path, mut diagnostic) in found {
        if diagnostic.source.is_none() {
            diagnostic.source = Some(source.to_owned());
        }
        let list = by_file.entry(path).or_default();
        // A tool may say the same twice (cargo, once per target).
        if !list.contains(&diagnostic) {
            list.push(diagnostic);
        }
    }
    Ok(by_file)
}

/// A path as a tool printed it: a `file://` address, absolute, or relative
/// to the root.
fn resolve(path: &str, root: &Path) -> PathBuf {
    if let Some(rest) = path.strip_prefix("file://") {
        return PathBuf::from(percent_decode(rest));
    }
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[index + 1..index + 3], 16)
        {
            out.push(byte);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A range from lines and columns counted from 1 (columns from
/// `column_base`); a missing end is the start's line end.
fn range_of(
    line: u32,
    column: Option<u32>,
    end_line: Option<u32>,
    end_column: Option<u32>,
    column_base: u32,
) -> Range {
    let line = line.saturating_sub(1);
    let character = column.map_or(0, |c| c.saturating_sub(column_base));
    let end_line_value = end_line.map_or(line, |l| l.saturating_sub(1));
    let end_character = match end_column {
        Some(c) => c.saturating_sub(column_base),
        None if end_line.is_none() => character.saturating_add(1).max(character),
        None => 0,
    };
    Range {
        start: Position { line, character },
        end: Position {
            line: end_line_value.max(line),
            character: if end_line_value > line {
                end_character
            } else {
                end_character.max(character)
            },
        },
    }
}

/// SARIF 2.1: every run's results, at their first physical location.
pub fn from_sarif(text: &str, root: &Path) -> Result<Vec<(PathBuf, Diagnostic)>, String> {
    let sarif: Value =
        serde_json::from_str(text.trim()).map_err(|error| format!("not SARIF: {error}"))?;
    let mut found = Vec::new();
    for run in sarif["runs"].as_array().into_iter().flatten() {
        let tool = run["tool"]["driver"]["name"].as_str().map(str::to_owned);
        let rules = run["tool"]["driver"]["rules"].as_array();
        for result in run["results"].as_array().into_iter().flatten() {
            let Some(location) = result["locations"][0]["physicalLocation"].as_object() else {
                continue;
            };
            let Some(uri) = location
                .get("artifactLocation")
                .and_then(|artifact| artifact["uri"].as_str())
            else {
                continue;
            };
            let region = location.get("region").cloned().unwrap_or(Value::Null);
            let number = |key: &str| region[key].as_u64().and_then(|n| u32::try_from(n).ok());
            let level = result["level"].as_str().unwrap_or("warning");
            let rule = result["ruleId"].as_str().map(str::to_owned).or_else(|| {
                let index = result["ruleIndex"].as_u64()?;
                rules?.get(usize::try_from(index).ok()?)?["id"]
                    .as_str()
                    .map(str::to_owned)
            });
            let message = result["message"]["text"]
                .as_str()
                .or_else(|| result["message"]["markdown"].as_str())
                .unwrap_or_default()
                .to_owned();
            found.push((
                resolve(uri, root),
                Diagnostic {
                    range: range_of(
                        number("startLine").unwrap_or(1),
                        number("startColumn"),
                        number("endLine"),
                        number("endColumn"),
                        1,
                    ),
                    severity: Some(match level {
                        "error" => ERROR,
                        "note" => INFORMATION,
                        "none" => HINT,
                        _ => WARNING,
                    }),
                    code: rule.map(Value::String),
                    source: tool.clone(),
                    message,
                    related_information: None,
                    tags: None,
                    data: None,
                },
            ));
        }
    }
    Ok(found)
}

/// One step of a JSON path.
enum Step<'a> {
    Up,
    Field(&'a str),
    Each,
    Find(&'a str, &'a str),
}

fn steps(path: &str) -> Vec<Step<'_>> {
    let mut steps = Vec::new();
    let mut rest = path;
    while let Some(after) = rest.strip_prefix("../") {
        steps.push(Step::Up);
        rest = after;
    }
    for part in rest.split('.').filter(|part| !part.is_empty()) {
        let (name, mut tail) = match part.find('[') {
            Some(at) => (&part[..at], &part[at..]),
            None => (part, ""),
        };
        if !name.is_empty() {
            steps.push(Step::Field(name));
        }
        while let Some(close) = tail.find(']') {
            let inside = &tail[1..close];
            match inside.split_once('=') {
                Some((key, value)) => steps.push(Step::Find(key, value)),
                None => steps.push(Step::Each),
            }
            tail = &tail[close + 1..];
        }
    }
    steps
}

/// The value at `path` from `chain` (the item last, those it was found in
/// before it).
fn lookup<'a>(chain: &[&'a Value], path: &str) -> Option<&'a Value> {
    let parsed = steps(path);
    let ups = parsed
        .iter()
        .take_while(|step| matches!(step, Step::Up))
        .count();
    let mut current = *chain.get(chain.len().checked_sub(1 + ups)?)?;
    for step in &parsed[ups..] {
        current = match step {
            Step::Up => return None,
            Step::Field(name) => current.get(*name)?,
            Step::Each => current.get(0)?,
            Step::Find(key, wanted) => {
                current.as_array()?.iter().find(|item| match &item[*key] {
                    Value::String(text) => text == wanted,
                    Value::Number(number) => {
                        number.as_f64().is_some_and(|n| wanted.parse() == Ok(n))
                    }
                    Value::Bool(flag) => wanted.parse() == Ok(*flag),
                    _ => false,
                })?
            }
        };
    }
    Some(current)
}

/// Every item `each` walks to from `document`, with the items it was found in.
fn walk<'a>(document: &'a Value, each: &str) -> Vec<Vec<&'a Value>> {
    // Each chain holds the items walked through (what `../` climbs) and
    // where the walk is now within the last one.
    let mut walks: Vec<(Vec<&'a Value>, &'a Value)> = vec![(vec![document], document)];
    for step in steps(each) {
        let mut next = Vec::new();
        for (chain, at) in walks {
            match step {
                Step::Field(name) => {
                    if let Some(value) = at.get(name) {
                        next.push((chain, value));
                    }
                }
                Step::Each => {
                    for item in at.as_array().into_iter().flatten() {
                        let mut chain = chain.clone();
                        chain.push(item);
                        next.push((chain, item));
                    }
                }
                Step::Find(..) | Step::Up => {}
            }
        }
        walks = next;
    }
    walks
        .into_iter()
        .map(|(mut chain, at)| {
            // A walk that ends on a field takes it as the item.
            if chain.last().is_none_or(|last| !std::ptr::eq(*last, at)) {
                chain.push(at);
            }
            chain
        })
        .collect()
}

fn as_number(value: Option<&Value>) -> Option<u32> {
    match value? {
        Value::Number(number) => number.as_u64().and_then(|n| u32::try_from(n).ok()),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn as_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// A declared JSON shape.
pub fn from_json(
    text: &str,
    shape: &JsonShape,
    root: &Path,
) -> Result<Vec<(PathBuf, Diagnostic)>, String> {
    let documents: Vec<Value> = if shape.lines {
        text.lines()
            .filter(|line| line.trim_start().starts_with(['{', '[']))
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    } else {
        vec![serde_json::from_str(text.trim()).map_err(|error| format!("not JSON: {error}"))?]
    };
    let column_base = shape.column_base.unwrap_or(1);
    let mut found = Vec::new();
    for document in &documents {
        for chain in walk(document, &shape.each) {
            if chain.is_empty() {
                continue;
            }
            if !shape
                .when
                .iter()
                .all(|(key, wanted)| lookup(&chain, key) == Some(wanted))
            {
                continue;
            }
            let (Some(path), Some(line), Some(message)) = (
                as_text(lookup(&chain, &shape.path)),
                as_number(lookup(&chain, &shape.line)),
                as_text(lookup(&chain, &shape.message)),
            ) else {
                continue;
            };
            let field =
                |name: &Option<String>| name.as_deref().and_then(|name| lookup(&chain, name));
            let severity = as_text(field(&shape.severity)).map(|word| {
                shape
                    .severities
                    .get(&word)
                    .and_then(|mapped| severity_of(mapped))
                    .or_else(|| severity_of(&word))
                    .unwrap_or(WARNING)
            });
            found.push((
                resolve(&path, root),
                Diagnostic {
                    range: range_of(
                        line,
                        as_number(field(&shape.column)),
                        as_number(field(&shape.end_line)),
                        as_number(field(&shape.end_column)),
                        column_base,
                    ),
                    severity: Some(severity.unwrap_or(WARNING)),
                    code: as_text(field(&shape.code)).map(Value::String),
                    source: None,
                    message,
                    related_information: None,
                    tags: None,
                    data: None,
                },
            ));
        }
    }
    Ok(found)
}

/// A line pattern: one problem per matching line.
pub fn from_pattern(
    text: &str,
    pattern: &Pattern,
    root: &Path,
) -> Result<Vec<(PathBuf, Diagnostic)>, String> {
    let regex = regex::Regex::new(&pattern.regexp).map_err(|error| error.to_string())?;
    let mut found = Vec::new();
    for line in text.lines() {
        let Some(captures) = regex.captures(line) else {
            continue;
        };
        let group = |name: &str| captures.name(name).map(|m| m.as_str().to_owned());
        let number = |name: &str| group(name).and_then(|text| text.parse::<u32>().ok());
        let (Some(file), Some(line_number)) = (group("file"), number("line")) else {
            continue;
        };
        let word = group("severity").or_else(|| pattern.severity.clone());
        let severity = word.as_deref().map_or(ERROR, |word| {
            pattern
                .severities
                .get(word)
                .and_then(|mapped| severity_of(mapped))
                .or_else(|| severity_of(word))
                .unwrap_or(WARNING)
        });
        found.push((
            resolve(file.trim(), root),
            Diagnostic {
                range: range_of(
                    line_number,
                    number("column"),
                    number("endLine"),
                    number("endColumn"),
                    1,
                ),
                severity: Some(severity),
                code: group("code").map(Value::String),
                source: None,
                message: group("message").unwrap_or_else(|| line.to_owned()),
                related_information: None,
                tags: None,
                data: None,
            },
        ));
    }
    Ok(found)
}

/// Counts by severity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub errors: u32,
    pub warnings: u32,
    pub information: u32,
    pub hints: u32,
}

impl Counts {
    pub fn add(&mut self, severity: u8) {
        match severity {
            WARNING => self.warnings += 1,
            INFORMATION => self.information += 1,
            HINT => self.hints += 1,
            _ => self.errors += 1,
        }
    }

    pub fn of<'a>(diagnostics: impl IntoIterator<Item = &'a Diagnostic>) -> Self {
        let mut counts = Self::default();
        for diagnostic in diagnostics {
            counts.add(diagnostic.severity());
        }
        counts
    }

    pub fn total(&self) -> u32 {
        self.errors + self.warnings + self.information + self.hints
    }
}

/// Problems by file and tool: a project's snapshot.
pub type Snapshot = BTreeMap<(String, String), Vec<Diagnostic>>;

/// What a turn changed in one file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDifference {
    pub path: String,
    /// The problems it now has that it did not have before.
    pub new: Vec<(String, Diagnostic)>,
    /// How many it had before that are gone.
    pub fixed: Counts,
}

/// "3 new errors, 1 fixed": what an agent's turn changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnSummary {
    pub new: Counts,
    pub fixed: Counts,
    pub files: Vec<FileDifference>,
}

/// The format of a stored turn summary (`turn_diagnostics.summary_format`).
pub const SUMMARY_FORMAT: i32 = 1;

/// The difference between two snapshots, problem by problem: a problem is
/// the same when its file, tool, severity, code and message are (it may
/// have moved). Hints are left out.
pub fn difference(before: &Snapshot, after: &Snapshot) -> TurnSummary {
    let mut files: BTreeMap<String, FileDifference> = BTreeMap::new();
    let mut summary = TurnSummary::default();
    let keys: std::collections::BTreeSet<&(String, String)> =
        before.keys().chain(after.keys()).collect();
    for key in keys {
        let (path, source) = key;
        let old = before.get(key).map(Vec::as_slice).unwrap_or_default();
        let new = after.get(key).map(Vec::as_slice).unwrap_or_default();
        let mut remaining: Vec<(u8, Option<String>, String)> = old
            .iter()
            .filter(|d| d.severity() < HINT)
            .map(Diagnostic::identity)
            .collect();
        let entry = || FileDifference {
            path: path.clone(),
            new: Vec::new(),
            fixed: Counts::default(),
        };
        for diagnostic in new.iter().filter(|d| d.severity() < HINT) {
            let identity = diagnostic.identity();
            if let Some(at) = remaining.iter().position(|known| *known == identity) {
                remaining.remove(at);
            } else {
                summary.new.add(diagnostic.severity());
                files
                    .entry(path.clone())
                    .or_insert_with(entry)
                    .new
                    .push((source.clone(), diagnostic.clone()));
            }
        }
        for (severity, _, _) in remaining {
            summary.fixed.add(severity);
            files
                .entry(path.clone())
                .or_insert_with(entry)
                .fixed
                .add(severity);
        }
    }
    summary.files = files.into_values().collect();
    summary
}

#[cfg(test)]
mod checkstyle {
    use super::*;

    #[test]
    fn reads_checkstyle_xml_as_checkstyle_ktlint_and_detekt_write_it() {
        let text = r#"<?xml version="1.0" encoding="UTF-8"?>
<checkstyle version="10.20">
  <file name="src/main/java/App.java">
    <error line="12" column="5" severity="warning" message="&apos;if&apos; is not followed by whitespace." source="com.puppycrawl.tools.checkstyle.checks.whitespace.WhitespaceAroundCheck"/>
    <error line="3" severity="error" message="Unused import &lt;List&gt;" source="com.puppycrawl.tools.checkstyle.checks.imports.UnusedImportsCheck"/>
    <error line="4" severity="ignore" message="hidden" source="x.Y"/>
  </file>
  <file name="/abs/Lib.kt"><error line="1" column="1" severity="info" message="Wildcard import" source="standard:no-wildcard-imports"/></file>
  <file name="Clean.java"/>
</checkstyle>"#;
        let found = from_checkstyle(text, Path::new("/w")).unwrap();
        assert_eq!(found.len(), 3);
        let (path, first) = &found[0];
        assert_eq!(path, Path::new("/w/src/main/java/App.java"));
        assert_eq!(
            first.range.start,
            Position {
                line: 11,
                character: 4
            }
        );
        assert_eq!(first.severity, Some(WARNING));
        assert_eq!(first.message, "'if' is not followed by whitespace.");
        assert_eq!(first.code_text().as_deref(), Some("WhitespaceAround"));
        assert_eq!(
            found[1].1.range.start,
            Position {
                line: 2,
                character: 0
            }
        );
        assert_eq!(found[1].1.message, "Unused import <List>");
        assert_eq!(found[2].0, Path::new("/abs/Lib.kt"));
        assert_eq!(found[2].1.severity, Some(INFORMATION));
        assert_eq!(
            found[2].1.code_text().as_deref(),
            Some("standard:no-wildcard-imports")
        );
        assert!(from_checkstyle("{}", Path::new("/w")).is_err());
        assert!(from_checkstyle("<other/>", Path::new("/w")).is_err());
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::modules::tools::registry::model::ToolRecord;

    fn bundled(id: &str) -> ToolRecord {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../tools/registry/{id}.json")),
        )
        .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn cargo_clippy_lines_become_diagnostics() {
        let output = [
            json!({ "reason": "compiler-artifact", "target": {} }).to_string(),
            json!({ "reason": "compiler-message", "message": {
                "level": "warning", "message": "unused variable: `x`",
                "code": { "code": "unused_variables" },
                "spans": [
                    { "file_name": "src/other.rs", "line_start": 1, "column_start": 1, "line_end": 1, "column_end": 2, "is_primary": false },
                    { "file_name": "src/main.rs", "line_start": 3, "column_start": 9, "line_end": 3, "column_end": 10, "is_primary": true }
                ]
            }})
            .to_string(),
            json!({ "reason": "compiler-message", "message": {
                "level": "error", "message": "aborting due to 1 previous error", "code": null, "spans": []
            }})
            .to_string(),
        ]
        .join("\n");
        let record = bundled("cargo-clippy");
        let found = read_output(
            record.run.output.as_ref().unwrap(),
            &output,
            Path::new("/w"),
            "cargo-clippy",
        )
        .unwrap();
        let diagnostics = &found[Path::new("/w/src/main.rs")];
        assert_eq!(found.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(WARNING));
        assert_eq!(
            diagnostics[0].code_text().as_deref(),
            Some("unused_variables")
        );
        assert_eq!(
            diagnostics[0].range,
            Range {
                start: Position {
                    line: 2,
                    character: 8
                },
                end: Position {
                    line: 2,
                    character: 9
                }
            }
        );
        assert_eq!(diagnostics[0].source.as_deref(), Some("cargo-clippy"));
    }

    #[test]
    fn tsc_lines_match_its_pattern() {
        let record = bundled("tsc");
        let output = "src/app.ts(12,5): error TS2322: Type 'string' is not assignable to type 'number'.\n\
                      Found 1 error.\n";
        let found = read_output(
            record.run.output.as_ref().unwrap(),
            output,
            Path::new("/w"),
            "tsc",
        )
        .unwrap();
        let diagnostic = &found[Path::new("/w/src/app.ts")][0];
        assert_eq!(diagnostic.code_text().as_deref(), Some("TS2322"));
        assert_eq!(
            diagnostic.range.start,
            Position {
                line: 11,
                character: 4
            }
        );
        assert_eq!(diagnostic.severity, Some(ERROR));
    }

    #[test]
    fn shellcheck_and_sarif_read_their_shapes() {
        let record = bundled("shellcheck");
        let output = json!({ "comments": [{
            "file": "/w/run.sh", "line": 2, "endLine": 2, "column": 6, "endColumn": 10,
            "level": "style", "code": 2086, "message": "Double quote to prevent globbing."
        }]})
        .to_string();
        let found = read_output(
            record.run.output.as_ref().unwrap(),
            &output,
            Path::new("/w"),
            "shellcheck",
        )
        .unwrap();
        let diagnostic = &found[Path::new("/w/run.sh")][0];
        assert_eq!(diagnostic.severity, Some(HINT));
        assert_eq!(diagnostic.code_text().as_deref(), Some("2086"));

        let sarif = json!({ "version": "2.1.0", "runs": [{
            "tool": { "driver": { "name": "hadolint", "rules": [{ "id": "DL3008" }] } },
            "results": [{
                "ruleIndex": 0, "level": "warning", "message": { "text": "Pin versions in apt get install." },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": "Dockerfile" },
                    "region": { "startLine": 4, "startColumn": 1 }
                }}]
            }]
        }]})
        .to_string();
        let found = read_output(
            &Output {
                format: OutputFormat::Sarif,
                stream: None,
                json: None,
                pattern: None,
                reports: Vec::new(),
            },
            &sarif,
            Path::new("/w"),
            "hadolint",
        )
        .unwrap();
        let diagnostic = &found[Path::new("/w/Dockerfile")][0];
        assert_eq!(diagnostic.code_text().as_deref(), Some("DL3008"));
        assert_eq!(diagnostic.source.as_deref(), Some("hadolint"));
        assert_eq!(diagnostic.range.start.line, 3);
    }

    #[test]
    fn nested_json_reaches_its_parents() {
        let shape: JsonShape = serde_json::from_value(json!({
            "each": "[].messages[]", "path": "../filePath", "line": "line", "column": "column",
            "severity": "severity", "code": "ruleId", "message": "message",
            "severities": { "2": "error", "1": "warning" }
        }))
        .unwrap();
        let eslint = json!([{ "filePath": "/w/a.js", "messages": [
            { "line": 1, "column": 7, "severity": 2, "ruleId": "no-unused-vars", "message": "'x' is unused." },
            { "line": 2, "column": 1, "severity": 1, "ruleId": "semi", "message": "Missing semicolon." }
        ]}])
        .to_string();
        let found = from_json(&eslint, &shape, Path::new("/w")).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, Path::new("/w/a.js"));
        assert_eq!(found[0].1.severity, Some(ERROR));
        assert_eq!(found[1].1.severity, Some(WARNING));
    }

    #[test]
    fn a_turn_counts_what_is_new_and_what_was_fixed() {
        let diagnostic = |severity: u8, message: &str, line: u32| Diagnostic {
            range: Range {
                start: Position { line, character: 0 },
                end: Position { line, character: 1 },
            },
            severity: Some(severity),
            code: None,
            source: None,
            message: message.to_owned(),
            related_information: None,
            tags: None,
            data: None,
        };
        let key = |path: &str| (path.to_owned(), "rust-analyzer".to_owned());
        let before = Snapshot::from([(
            key("a.rs"),
            vec![diagnostic(ERROR, "old", 1), diagnostic(WARNING, "kept", 2)],
        )]);
        let after = Snapshot::from([
            // The kept warning moved; it is the same problem.
            (key("a.rs"), vec![diagnostic(WARNING, "kept", 9)]),
            (
                key("b.rs"),
                vec![
                    diagnostic(ERROR, "new", 1),
                    diagnostic(ERROR, "new too", 2),
                    diagnostic(HINT, "hint", 3),
                ],
            ),
        ]);
        let summary = difference(&before, &after);
        assert_eq!(summary.new.errors, 2);
        assert_eq!(summary.fixed.errors, 1);
        assert_eq!(summary.files.len(), 2);
        assert_eq!(summary.files[1].path, "b.rs");
        assert_eq!(summary.files[1].new.len(), 2);
    }
}
