//! Searching on the machine, with ripgrep's libraries: the walk follows
//! `.gitignore` and friends, binary and large files are skipped, and the
//! results are bounded.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;

use crate::modules::projects::search::api::{
    FileIndexDto, FileMatchesDto, MatchDto, SearchQueryDto, SearchResultsDto,
};
use crate::shared::error::{DaemonError, fail};
use crate::shared::sync::lock;

/// At most this many matches per search.
pub const MAX_MATCHES: u32 = 2000;
/// At most this many matches per file.
pub const MAX_PER_FILE: usize = 200;
/// Files above this size are not searched.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// A search stops after this long.
pub const DEADLINE: Duration = Duration::from_secs(10);
/// At most this many paths in the quick open list.
pub const MAX_INDEX: usize = 100_000;
/// A quick open list is made again when older than this.
const INDEX_FRESH: Duration = Duration::from_secs(30);
/// Characters of a long line shown around its first match.
const PREVIEW_AROUND: usize = 80;

fn invalid(message: impl Into<String>) -> DaemonError {
    fail("invalid_pattern", message)
}

/// A walk of `root` with the query's rules.
fn walk(root: &Path, query: &SearchQueryDto) -> Result<WalkBuilder, DaemonError> {
    let mut overrides = OverrideBuilder::new(root);
    for glob in query
        .include
        .iter()
        .map(|g| g.trim())
        .filter(|g| !g.is_empty())
    {
        overrides
            .add(glob)
            .map_err(|error| invalid(format!("{glob}: {error}")))?;
    }
    for glob in query
        .exclude
        .iter()
        .map(|g| g.trim())
        .filter(|g| !g.is_empty())
    {
        overrides
            .add(&format!("!{glob}"))
            .map_err(|error| invalid(format!("{glob}: {error}")))?;
    }
    let overrides = overrides
        .build()
        .map_err(|error| invalid(error.to_string()))?;
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(!query.include_ignored)
        .git_ignore(!query.include_ignored)
        .git_global(!query.include_ignored)
        .git_exclude(!query.include_ignored)
        .ignore(!query.include_ignored)
        .parents(true)
        .follow_links(false)
        .max_filesize(Some(MAX_FILE_BYTES))
        .overrides(overrides)
        // In order, so streamed results arrive as they will be shown.
        .sort_by_file_name(std::cmp::Ord::cmp)
        // `.git` itself is never searched.
        .filter_entry(|entry| entry.file_name() != ".git");
    Ok(builder)
}

/// Character offsets of `[start, end)` byte offsets in `text`.
fn chars(text: &str, byte: usize) -> u32 {
    u32::try_from(text[..byte.min(text.len())].chars().count()).unwrap_or(u32::MAX)
}

/// A line as shown: trimmed of its newline, cut around the first match when
/// long, with the matches' places in it.
fn preview(line: &str, matches: &[(usize, usize)]) -> (String, Vec<[u32; 2]>) {
    let line = line.trim_end_matches(['\n', '\r']);
    let length = line.chars().count();
    let first = matches
        .first()
        .map_or(0, |(start, _)| chars(line, *start) as usize);
    let (from, to) = if length > PREVIEW_AROUND * 3 {
        let from = first.saturating_sub(PREVIEW_AROUND);
        (from, (from + PREVIEW_AROUND * 3).min(length))
    } else {
        (0, length)
    };
    let shown: String = line.chars().skip(from).take(to - from).collect();
    let offset = u32::try_from(from).unwrap_or(u32::MAX);
    let limit = u32::try_from(to - from).unwrap_or(u32::MAX);
    let ranges: Vec<[u32; 2]> = matches
        .iter()
        .map(|(start, end)| {
            let start = chars(line, *start).saturating_sub(offset).min(limit);
            let end = chars(line, *end).saturating_sub(offset).min(limit);
            [start, end]
        })
        .filter(|[start, end]| end > start)
        .collect();
    let prefix = if from > 0 { "…" } else { "" };
    let suffix = if to < length { "…" } else { "" };
    let ranges = if prefix.is_empty() {
        ranges
    } else {
        ranges
            .into_iter()
            .map(|[start, end]: [u32; 2]| [start + 1, end + 1])
            .collect()
    };
    (format!("{prefix}{shown}{suffix}"), ranges)
}

/// The query's matcher; an error when its pattern does not parse.
fn matcher(query: &SearchQueryDto) -> Result<grep_regex::RegexMatcher, DaemonError> {
    RegexMatcherBuilder::new()
        .case_insensitive(!query.match_case)
        .word(query.whole_word)
        .fixed_strings(!query.regex)
        .build(&query.query)
        .map_err(|error| invalid(error.to_string()))
}

/// Whether `query` can run in `root`: its pattern and globs parse.
pub fn check(root: &Path, query: &SearchQueryDto) -> Result<(), DaemonError> {
    if query.query.is_empty() {
        return Ok(());
    }
    matcher(query)?;
    walk(root, query).map(|_| ())
}

/// Searches `root`'s files for `query`.
pub fn search(root: &Path, query: &SearchQueryDto) -> Result<SearchResultsDto, DaemonError> {
    let mut files = Vec::new();
    let (total, cut) = search_each(root, query, |file| {
        files.push(file);
        true
    })?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(SearchResultsDto { files, total, cut })
}

/// Searches `root`'s files for `query`, handing each file's matches to
/// `found` as they are found, in path order; `found` answers false to stop
/// (nobody wants the rest). Answers the matches' count and why it stopped
/// early, if it did.
pub fn search_each(
    root: &Path,
    query: &SearchQueryDto,
    mut found: impl FnMut(FileMatchesDto) -> bool,
) -> Result<(u32, Option<String>), DaemonError> {
    if query.query.is_empty() {
        return Ok((0, None));
    }
    let matcher = matcher(query)?;
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .line_number(true)
        .build();
    let started = Instant::now();
    let mut total: u32 = 0;
    let mut cut = None;
    for entry in walk(root, query)?.build() {
        if total >= MAX_MATCHES {
            cut = Some("matches".to_owned());
            break;
        }
        if started.elapsed() > DEADLINE {
            cut = Some("time".to_owned());
            break;
        }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        let mut matches = Vec::new();
        let mut more = false;
        let budget = (MAX_MATCHES - total) as usize;
        let result = searcher.search_path(
            &matcher,
            path,
            UTF8(|line_number, line| {
                if matches.len() >= MAX_PER_FILE.min(budget) {
                    more = true;
                    return Ok(false);
                }
                let mut found = Vec::new();
                matcher
                    .find_iter(line.as_bytes(), |m| {
                        found.push((m.start(), m.end()));
                        true
                    })
                    .ok();
                let (preview, ranges) = preview(line, &found);
                matches.push(MatchDto {
                    line: line_number,
                    preview,
                    ranges,
                });
                Ok(true)
            }),
        );
        if result.is_err() || matches.is_empty() {
            continue;
        }
        total += u32::try_from(matches.len()).unwrap_or(u32::MAX);
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if !found(FileMatchesDto {
            path: relative,
            matches,
            more,
        }) {
            break;
        }
    }
    if cut.is_none() && total >= MAX_MATCHES {
        cut = Some("matches".to_owned());
    }
    Ok((total, cut))
}

/// The project's files, for quick open.
pub fn index(root: &Path) -> FileIndexDto {
    let mut paths = Vec::new();
    let mut truncated = false;
    let mut builder = WalkBuilder::new(root);
    builder
        .follow_links(false)
        .filter_entry(|entry| entry.file_name() != ".git");
    for entry in builder.build().flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        if paths.len() >= MAX_INDEX {
            truncated = true;
            break;
        }
        if let Ok(relative) = entry.path().strip_prefix(root) {
            paths.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    paths.sort();
    FileIndexDto { paths, truncated }
}

/// Quick open lists, kept a little while per folder.
#[derive(Default)]
pub struct Indexes(Mutex<HashMap<std::path::PathBuf, (Instant, FileIndexDto)>>);

impl Indexes {
    /// The folder's list, made again when stale.
    pub fn get(&self, root: &Path) -> FileIndexDto {
        if let Some((made, index)) = lock(&self.0).get(root)
            && made.elapsed() < INDEX_FRESH
        {
            return index.clone();
        }
        let fresh = index(root);
        lock(&self.0).insert(root.to_path_buf(), (Instant::now(), fresh.clone()));
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    println!(\"Hello\");\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn hello() {}\n// hello again\n",
        )
        .unwrap();
        std::fs::write(root.join("target/out.rs"), "fn hello() {}\n").unwrap();
        std::fs::write(root.join("blob.bin"), b"hello\x00world").unwrap();
        // The ignore rules apply inside a Git repository.
        std::fs::create_dir_all(root.join(".git")).unwrap();
        (dir, root)
    }

    fn find(root: &Path, query: SearchQueryDto) -> SearchResultsDto {
        search(root, &query).unwrap()
    }

    #[test]
    fn searches_text_files_following_ignore_rules() {
        let (_dir, root) = folder();
        let results = find(
            &root,
            SearchQueryDto {
                query: "hello".into(),
                ..Default::default()
            },
        );
        let paths: Vec<_> = results.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            ["src/lib.rs", "src/main.rs"],
            "not target/, not the binary"
        );
        assert_eq!(results.total, 3);
        let lib = &results.files[0];
        assert_eq!(lib.matches[0].line, 1);
        assert_eq!(lib.matches[0].ranges, vec![[7, 12]]);

        let ignored = find(
            &root,
            SearchQueryDto {
                query: "hello".into(),
                include_ignored: true,
                ..Default::default()
            },
        );
        assert!(ignored.files.iter().any(|f| f.path == "target/out.rs"));
    }

    #[test]
    fn honours_case_words_regexes_and_patterns() {
        let (_dir, root) = folder();
        let exact = find(
            &root,
            SearchQueryDto {
                query: "Hello".into(),
                match_case: true,
                ..Default::default()
            },
        );
        assert_eq!(exact.total, 1);
        let word = find(
            &root,
            SearchQueryDto {
                query: "hell".into(),
                whole_word: true,
                ..Default::default()
            },
        );
        assert_eq!(word.total, 0);
        let regex = find(
            &root,
            SearchQueryDto {
                query: r"fn \w+\(\)".into(),
                regex: true,
                ..Default::default()
            },
        );
        assert_eq!(regex.total, 2);
        let only = find(
            &root,
            SearchQueryDto {
                query: "hello".into(),
                include: vec!["**/lib.rs".into()],
                ..Default::default()
            },
        );
        assert_eq!(only.files.len(), 1);
        let not = find(
            &root,
            SearchQueryDto {
                query: "hello".into(),
                exclude: vec!["**/lib.rs".into()],
                ..Default::default()
            },
        );
        assert_eq!(not.files[0].path, "src/main.rs");
        assert_eq!(
            search(
                &root,
                &SearchQueryDto {
                    query: "(".into(),
                    regex: true,
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
            "invalid_pattern"
        );
    }

    #[test]
    fn long_lines_show_around_the_match() {
        let line = format!("{}needle{}", "a".repeat(500), "b".repeat(500));
        let (shown, ranges) = preview(&line, &[(500, 506)]);
        assert!(shown.starts_with('…') && shown.ends_with('…'));
        let [start, end] = ranges[0];
        let picked: String = shown
            .chars()
            .skip(start as usize)
            .take((end - start) as usize)
            .collect();
        assert_eq!(picked, "needle");
    }

    #[test]
    fn lists_files_for_quick_open() {
        let (_dir, root) = folder();
        let index = index(&root);
        assert!(index.paths.contains(&"src/main.rs".to_owned()));
        assert!(!index.paths.iter().any(|p| p.starts_with("target/")));
        assert!(!index.paths.iter().any(|p| p.starts_with(".git")));
    }
}
