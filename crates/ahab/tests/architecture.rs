//! The layout and import rules, checked against the source.
//!
//! - `src/` holds only the entry points, the plumbing (`cli`, `config`,
//!   `server`, `daemon`), the features (`modules`) and `shared`.
//! - `modules/` holds only the modules in `MODULES`; a module's root holds
//!   only `mod.rs` and component folders; a component uses only the allowed
//!   file names.
//! - A component uses another module only through that module's `mod.rs`,
//!   only its `model` and `service` items, and only modules listed for it.
//! - Only the thin layers use plumbing: a component's `http` uses the
//!   server's errors, extractors and state; its `cli` uses the command line's
//!   client, exit codes, output and prompts; a module's `mod.rs` may name the
//!   routes' state to merge its routes. `model`, `service`, `store` and `api`
//!   use none of it.
//! - `shared` uses nothing else; `model` files do no I/O.

// Clippy allows unwrap in test functions but not in their helpers.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is a failed test")]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The modules, and the modules each may use. A new module goes here, after
/// the ones it uses.
const MODULES: [(&str, &[&str]); 8] = [
    ("platform", &[]),
    ("clients", &[]),
    ("machines", &[]),
    ("projects", &["machines"]),
    ("sessions", &["machines", "projects"]),
    ("plugins", &["machines", "projects"]),
    ("tools", &["machines", "projects", "sessions", "plugins"]),
    (
        "notifications",
        &["clients", "machines", "projects", "sessions", "plugins"],
    ),
];

/// What `src/` may hold.
const SRC: [&str; 9] = [
    "lib.rs",
    "main.rs",
    "cli",
    "config",
    "daemon",
    "server",
    "modules",
    "shared",
    "integrations",
];

/// What a component's root may hold, as `<name>.rs` or a `<name>/` folder.
const COMPONENT_FILES: [&str; 8] = [
    "model", "service", "store", "api", "http", "cli", "daemon", "tests",
];

/// The plumbing each thin layer may use: (layer file, allowed paths). A
/// module's own `mod.rs` merges its components' routes, so it may name the
/// routes' state.
const PLUMBING: [(&str, &[&str]); 3] = [
    ("mod", &["server::state::AppState"]),
    (
        "http",
        &[
            "server::error",
            "server::extract",
            "server::state::AppState",
        ],
    ),
    (
        "cli",
        &["cli::client", "cli::exit", "cli::output", "cli::prompt"],
    ),
];

/// The top-level plumbing folders a module file may never reach otherwise.
const PLUMBING_ROOTS: [&str; 4] = ["cli", "config", "server", "daemon"];

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn modules_dir() -> PathBuf {
    src().join("modules")
}

/// The components of a module: its folders.
fn components(module: &str) -> Vec<String> {
    entries(&modules_dir().join(module))
        .into_iter()
        .filter(|(_, is_dir)| *is_dir)
        .map(|(name, _)| name)
        .collect()
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![dir.to_owned()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn entries(dir: &Path) -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.path().is_dir(),
            )
        })
        .collect();
    entries.sort();
    entries
}

/// A path relative to `src/`.
fn relative(path: &Path) -> String {
    path.strip_prefix(src())
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

/// For a file in `modules/<module>/<component>/…`: its module, and the
/// component file it belongs to (`http` for `http.rs` or `http/…`). A
/// module's own `mod.rs` is `mod`.
fn place(path: &Path) -> (String, Option<String>) {
    let rel = path.strip_prefix(modules_dir()).unwrap();
    let parts: Vec<String> = rel
        .iter()
        .map(|part| part.to_string_lossy().trim_end_matches(".rs").to_owned())
        .collect();
    let module = parts.first().cloned().unwrap_or_default();
    let file = match parts.len() {
        2 => parts.get(1).cloned(),
        _ => parts.get(2).cloned(),
    };
    (module, file)
}

/// The source without comments, string contents and test modules, so paths
/// in them are not taken for imports. Tests wire what they need, as the
/// server's startup does.
fn code(text: &str) -> String {
    without_tests(&uncommented(text))
}

/// The source without its `#[cfg(test)]` modules.
fn without_tests(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    while let Some(at) = rest.find("#[cfg(test)]") {
        let after = &rest[at + "#[cfg(test)]".len()..];
        let is_module = after.trim_start().starts_with("mod ");
        let Some(open) = after.find('{').filter(|_| is_module) else {
            out.push_str(&rest[..at + "#[cfg(test)]".len()]);
            rest = after;
            continue;
        };
        out.push_str(&rest[..at]);
        let mut depth = 0;
        let mut end = after.len();
        for (index, c) in after[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + index + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        // Keep the lines, so line numbers stay right.
        out.extend(after[..end].matches('\n').map(|_| '\n'));
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn uncommented(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '"' => {
                out.push('"');
                let mut escaped = false;
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                    }
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        break;
                    }
                }
                out.push('"');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Every `crate::…` path in the code, with `use` groups expanded: each as
/// its segments after `crate`.
fn crate_paths(code: &str) -> Vec<(usize, Vec<String>)> {
    let mut paths = Vec::new();
    let bytes = code.as_bytes();
    let mut at = 0;
    while let Some(found) = code[at..].find("crate::") {
        let start = at + found;
        at = start + "crate::".len();
        if start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
            continue;
        }
        let line = code[..start].matches('\n').count() + 1;
        let (expanded, end) = expand(code, at);
        at = end;
        for path in expanded {
            paths.push((line, path));
        }
    }
    paths
}

/// Expands the path, or `use` tree, that starts at `at`.
fn expand(code: &str, mut at: usize) -> (Vec<Vec<String>>, usize) {
    let mut prefix = Vec::new();
    let bytes = code.as_bytes();
    loop {
        let start = at;
        while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
            at += 1;
        }
        if at > start {
            prefix.push(code[start..at].to_owned());
        }
        let rest = &code[at..];
        let trimmed = rest.trim_start();
        let skipped = rest.len() - trimmed.len();
        if trimmed.starts_with("::") {
            at += skipped + 2;
            let after = code[at..].trim_start();
            at += code[at..].len() - after.len();
            if after.starts_with('{') {
                let (group, end) = group(code, at + 1);
                let paths = group
                    .into_iter()
                    .map(|path| prefix.iter().cloned().chain(path).collect())
                    .collect();
                return (paths, end);
            }
            continue;
        }
        return (vec![prefix], at);
    }
}

/// The paths in a `{…}` group that starts at `at`, and where it ends.
fn group(code: &str, mut at: usize) -> (Vec<Vec<String>>, usize) {
    let mut paths = Vec::new();
    let bytes = code.as_bytes();
    loop {
        while at < bytes.len() && (bytes[at].is_ascii_whitespace() || bytes[at] == b',') {
            at += 1;
        }
        if at >= bytes.len() || bytes[at] == b'}' {
            return (paths, at + 1);
        }
        if bytes[at] == b'*' {
            paths.push(vec!["*".to_owned()]);
            at += 1;
            continue;
        }
        let (expanded, end) = expand(code, at);
        paths.extend(expanded.into_iter().filter(|path| !path.is_empty()));
        at = end.max(at + 1);
        // `as name`
        let rest = code[at..].trim_start();
        if let Some(alias) = rest.strip_prefix("as ") {
            let alias = alias.trim_start();
            let len = alias
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(alias.len());
            at = code.len() - alias.len() + len;
        }
    }
}

#[test]
fn src_holds_only_the_layout() {
    let problems: Vec<String> = entries(&src())
        .into_iter()
        .map(|(name, _)| name)
        .filter(|name| !SRC.contains(&name.as_str()))
        .collect();
    assert!(problems.is_empty(), "not part of the layout: {problems:#?}");
}

#[test]
fn modules_holds_only_the_listed_modules() {
    let known: Vec<&str> = MODULES.iter().map(|(module, _)| *module).collect();
    let problems: Vec<String> = entries(&modules_dir())
        .into_iter()
        .filter(|(name, is_dir)| {
            if *is_dir {
                !known.contains(&name.as_str())
            } else {
                name != "mod.rs"
            }
        })
        .map(|(name, _)| format!("modules/{name}"))
        .collect();
    assert!(
        problems.is_empty(),
        "not a module listed in MODULES: {problems:#?}"
    );
}

#[test]
fn module_roots_hold_only_mod_rs_and_components() {
    let mut problems = Vec::new();
    for (module, _) in MODULES {
        for (name, is_dir) in entries(&modules_dir().join(module)) {
            if !is_dir && name != "mod.rs" {
                problems.push(format!("modules/{module}/{name}"));
            }
        }
    }
    assert!(problems.is_empty(), "not a component folder: {problems:#?}");
}

#[test]
fn components_use_only_the_allowed_names() {
    let mut problems = Vec::new();
    for (module, _) in MODULES {
        for component in components(module) {
            let root = modules_dir().join(module).join(&component);
            if !root.join("mod.rs").exists() {
                problems.push(format!("modules/{module}/{component}/ has no mod.rs"));
            }
            for (name, is_dir) in entries(&root) {
                let stem = name.trim_end_matches(".rs");
                let allowed = if is_dir {
                    COMPONENT_FILES.contains(&name.as_str())
                } else {
                    name == "mod.rs" || COMPONENT_FILES.contains(&stem)
                };
                if !allowed {
                    problems.push(format!("modules/{module}/{component}/{name}"));
                } else if is_dir && !root.join(&name).join("mod.rs").exists() {
                    problems.push(format!(
                        "modules/{module}/{component}/{name}/ has no mod.rs"
                    ));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "names a component may not use: {problems:#?}"
    );
}

#[test]
fn modules_reexport_only_model_and_service_items() {
    let mut problems = Vec::new();
    for (module, _) in MODULES {
        let path = modules_dir().join(module).join("mod.rs");
        let code = code(&fs::read_to_string(&path).unwrap());
        for statement in code.split(';') {
            let statement = statement.trim();
            if !statement.starts_with("pub use ") {
                continue;
            }
            let (paths, _) = expand(statement, "pub use ".len());
            for path in paths {
                if !matches!(path.get(1).map(String::as_str), Some("model" | "service")) {
                    problems.push(format!("{module}/mod.rs: pub use {}", path.join("::")));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "re-exports of more than model and service items: {problems:#?}"
    );
}

#[test]
fn modules_use_other_modules_and_plumbing_only_as_allowed() {
    let allowed: BTreeMap<&str, &[&str]> = MODULES.into_iter().collect();
    let mut problems = Vec::new();
    for file in rust_files(&modules_dir()) {
        let (from, layer) = place(&file);
        let Some(uses) = allowed.get(from.as_str()) else {
            continue;
        };
        let code = code(&fs::read_to_string(&file).unwrap());
        for (line, path) in crate_paths(&code) {
            let joined = path.join("::");
            let at = format!("{}:{line}: crate::{joined}", relative(&file));
            let Some(first) = path.first().map(String::as_str) else {
                continue;
            };
            if first == "shared" {
                continue;
            }
            if PLUMBING_ROOTS.contains(&first) {
                let permitted = PLUMBING
                    .iter()
                    .filter(|(file, _)| layer.as_deref() == Some(*file))
                    .flat_map(|(_, paths)| paths.iter())
                    .any(|allowed| joined.starts_with(allowed));
                if !permitted {
                    problems.push(format!(
                        "{at} (only a component's http or cli uses its plumbing)"
                    ));
                }
                continue;
            }
            if first != "modules" {
                continue;
            }
            let Some(to) = path.get(1).map(String::as_str) else {
                continue;
            };
            if to == from {
                continue;
            }
            if !uses.contains(&to) {
                problems.push(format!("{at} ({from} may not use {to})"));
                continue;
            }
            // Through the module's mod.rs: `crate::modules::machines::Machine`,
            // never `crate::modules::machines::machine::model::…`.
            if path.len() < 3 || components(to).contains(&path[2]) {
                problems.push(format!("{at} (only through {to}/mod.rs)"));
            }
        }
    }
    assert!(problems.is_empty(), "imports in modules: {problems:#?}");
}

#[test]
fn shared_depends_on_nothing_else() {
    let mut problems = Vec::new();
    for file in rust_files(&src().join("shared")) {
        let code = code(&fs::read_to_string(&file).unwrap());
        for (line, path) in crate_paths(&code) {
            if path.first().is_some_and(|to| to != "shared") {
                problems.push(format!(
                    "{}:{line}: crate::{}",
                    relative(&file),
                    path.join("::")
                ));
            }
        }
    }
    assert!(problems.is_empty(), "shared uses: {problems:#?}");
}

/// A module's routes reach other modules' services through `AppState` too:
/// those must be modules it may use.
#[test]
fn routes_use_the_services_of_modules_they_may_use() {
    let allowed: BTreeMap<&str, &[&str]> = MODULES.into_iter().collect();
    let state = code(&fs::read_to_string(src().join("server/state.rs")).unwrap());
    // `pub name: Arc<Type>` with `use crate::modules::<module>::…::Type;`
    let imports: BTreeMap<String, String> = crate_paths(&state)
        .into_iter()
        .filter(|(_, path)| path.first().is_some_and(|first| first == "modules"))
        .filter_map(|(_, path)| Some((path.last()?.clone(), path.get(1)?.clone())))
        .collect();
    let mut owners: BTreeMap<String, String> = BTreeMap::new();
    for line in state.lines() {
        let Some(field) = line.trim().strip_prefix("pub ") else {
            continue;
        };
        let Some((name, kind)) = field.split_once(':') else {
            continue;
        };
        let kind = kind.trim().trim_end_matches(',');
        let inner = kind
            .strip_prefix("Arc<")
            .and_then(|kind| kind.strip_suffix('>'))
            .unwrap_or(kind);
        let inner = inner.rsplit("::").next().unwrap_or(inner);
        if let Some(module) = imports.get(inner) {
            owners.insert(name.trim().to_owned(), module.clone());
        }
    }
    // Until a module adds its services, there is nothing to check.
    let mut problems = Vec::new();
    for file in rust_files(&modules_dir()) {
        let (from, _) = place(&file);
        let Some(uses) = allowed.get(from.as_str()) else {
            continue;
        };
        let code = code(&fs::read_to_string(&file).unwrap());
        for (number, line) in code.lines().enumerate() {
            let mut rest = line;
            while let Some(at) = rest.find("state.") {
                let before = rest[..at].chars().last();
                rest = &rest[at + "state.".len()..];
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if let Some(owner) = owners.get(&name)
                    && *owner != from
                    && !uses.contains(&owner.as_str())
                {
                    problems.push(format!(
                        "{}:{}: state.{name} ({from} may not use {owner})",
                        relative(&file),
                        number + 1
                    ));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "services used across modules: {problems:#?}"
    );
}

#[test]
fn models_do_no_io() {
    let mut problems = Vec::new();
    for file in rust_files(&modules_dir()) {
        if place(&file).1.as_deref() != Some("model") {
            continue;
        }
        let rel = relative(&file);
        let code = code(&fs::read_to_string(&file).unwrap());
        for banned in ["sqlx", "axum", "reqwest", "tokio::fs"] {
            for (number, line) in code.lines().enumerate() {
                let found = line.match_indices(banned).any(|(at, _)| {
                    let before = line[..at].chars().last();
                    let after = line[at + banned.len()..].chars().next();
                    !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                        && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
                });
                if found {
                    problems.push(format!("{rel}:{}: {banned}", number + 1));
                }
            }
        }
    }
    assert!(problems.is_empty(), "I/O in models: {problems:#?}");
}

/// Tests make their folders with `tempfile` (or [`support::TestServer`]'s
/// `daemon_dir`), which removes them: a folder left in the temporary folder
/// keeps a terminal host started there running, with its shells, forever.
#[test]
fn tests_leave_no_folders_behind() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let marker = ["std::env::", "temp_dir()"].concat();
    let problems: Vec<String> = rust_files(&tests)
        .into_iter()
        .filter(|path| fs::read_to_string(path).unwrap().contains(&marker))
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        problems.is_empty(),
        "folders made by hand in the temporary folder are never removed; use tempfile or TestServer::daemon_dir: {problems:?}"
    );
}
