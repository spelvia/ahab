//! Paths between a language server (absolute `file://` addresses) and the
//! web app (paths in the project's folder). What a server returns outside
//! the folder becomes an external file, served only by grant: a token per
//! result, expiring with the server.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// The prefix of an external file's token in paths the web app sees.
pub const EXTERNAL: &str = "external:";

/// Something a server pointed at outside the project's folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(PathBuf),
    /// An address that is not a file (`jdt://…`), fetched from the server.
    Address(String),
}

/// How an external file is shown.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalInfo {
    /// Its origin: "rust std", "`node_modules/react`", "crate serde-1.0.228".
    pub label: String,
    /// Its name, for the window's tab.
    pub name: String,
    /// Where it is, for the window's path bar.
    pub display: String,
}

/// `path` as a `file://` address.
pub fn uri_of(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The path of a `file://` address.
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // `file://localhost/…` and `file:///…` both name local files.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&rest[index + 1..index + 3], 16)
        {
            out.push(byte);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    Some(PathBuf::from(String::from_utf8_lossy(&out).into_owned()))
}

/// Maps a server's answer for one project: its files become their
/// relative path, others a grant `external:<token>` (with `grant` making
/// the token), and `externals` says how to show them.
pub struct Mapper<'a> {
    pub folder: &'a Path,
    pub grant: &'a mut dyn FnMut(Target) -> String,
    pub externals: BTreeMap<String, ExternalInfo>,
}

impl Mapper<'_> {
    /// One address, as the web app sees it.
    pub fn place(&mut self, uri: &str) -> String {
        if let Some(path) = path_of(uri) {
            if let Ok(relative) = path.strip_prefix(self.folder) {
                return relative.to_string_lossy().into_owned();
            }
            let token = (self.grant)(Target::File(path.clone()));
            self.externals.insert(token.clone(), describe(&path));
            return format!("{EXTERNAL}{token}");
        }
        let token = (self.grant)(Target::Address(uri.to_owned()));
        self.externals.insert(token.clone(), describe_address(uri));
        format!("{EXTERNAL}{token}")
    }

    /// Every address in `value` mapped: `uri`, `targetUri`, `oldUri` and
    /// `newUri` fields, and a workspace edit's `changes` keys.
    pub fn map(&mut self, value: &mut Value) {
        match value {
            Value::Object(map) => {
                for key in ["uri", "targetUri", "oldUri", "newUri"] {
                    if let Some(Value::String(uri)) = map.get(key) {
                        let mapped = self.place(&uri.clone());
                        map.insert(key.to_owned(), Value::String(mapped));
                    }
                }
                if let Some(Value::Object(changes)) = map.get_mut("changes") {
                    let old = std::mem::take(changes);
                    let mut mapped = Map::new();
                    for (uri, mut edits) in old {
                        self.map(&mut edits);
                        mapped.insert(self.place(&uri), edits);
                    }
                    *changes = mapped;
                }
                for (key, child) in map.iter_mut() {
                    if key != "changes" {
                        self.map(child);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.map(item);
                }
            }
            _ => {}
        }
    }
}

/// How to show a file outside the project's folder, by what its path says.
pub fn describe(path: &Path) -> ExternalInfo {
    let text = path.to_string_lossy();
    let name = path.file_name().map_or_else(
        || text.clone().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let label = if let Some(at) = text.find("/library/") {
        let rest = &text[at + "/library/".len()..];
        if text.contains("/rustlib/") {
            format!("rust {}", rest.split('/').next().unwrap_or("std"))
        } else {
            "library".to_owned()
        }
    } else if let Some(at) = text.rfind("/node_modules/") {
        let rest = &text[at + "/node_modules/".len()..];
        let mut parts = rest.split('/');
        let first = parts.next().unwrap_or_default();
        let package = if first.starts_with('@') {
            format!("{first}/{}", parts.next().unwrap_or_default())
        } else {
            first.to_owned()
        };
        format!("node_modules/{package}")
    } else if let Some(at) = text.find("/.cargo/registry/src/") {
        let rest = &text[at + "/.cargo/registry/src/".len()..];
        let krate = rest.split('/').nth(1).unwrap_or("crate");
        format!("crate {krate}")
    } else if let Some(at) = text.find("/.cargo/git/checkouts/") {
        let rest = &text[at + "/.cargo/git/checkouts/".len()..];
        format!("git {}", rest.split('/').next().unwrap_or("crate"))
    } else if let Some(at) = text.find("/site-packages/") {
        let rest = &text[at + "/site-packages/".len()..];
        format!("python {}", rest.split('/').next().unwrap_or("package"))
    } else if text.contains("/go/pkg/mod/") {
        let rest = text.split("/go/pkg/mod/").nth(1).unwrap_or_default();
        format!("go {}", rest.split('@').next().unwrap_or("module"))
    } else if text.contains("/nix/store/") {
        "Nix store".to_owned()
    } else {
        path.parent()
            .and_then(|parent| parent.file_name())
            .map_or_else(
                || "outside the project".to_owned(),
                |parent| parent.to_string_lossy().into_owned(),
            )
    };
    ExternalInfo {
        label,
        name,
        display: text.into_owned(),
    }
}

/// How to show an address that is not a file: `jdt://contents/…/String.class`.
pub fn describe_address(uri: &str) -> ExternalInfo {
    let scheme = uri.split(':').next().unwrap_or("address");
    let path = uri.split('?').next().unwrap_or(uri);
    let name = path.rsplit('/').next().unwrap_or(path).to_owned();
    let label = match scheme {
        "jdt" => "JDK or a library (from jdtls)".to_owned(),
        other => format!("{other} (from the language server)"),
    };
    ExternalInfo {
        label,
        name,
        display: uri.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn addresses_round_trip() {
        let path = Path::new("/work/my app/src/lib.rs");
        let uri = uri_of(path);
        assert_eq!(uri, "file:///work/my%20app/src/lib.rs");
        assert_eq!(path_of(&uri).unwrap(), path);
        assert_eq!(
            path_of("file://localhost/a%2Fb").unwrap(),
            Path::new("/a/b")
        );
    }

    #[test]
    fn answers_name_project_files_and_grant_the_rest() {
        let mut granted = Vec::new();
        let mut grant = |target: Target| {
            granted.push(target);
            format!("t{}", granted.len())
        };
        let mut mapper = Mapper {
            folder: Path::new("/work/app"),
            grant: &mut grant,
            externals: BTreeMap::new(),
        };
        let mut answer = json!([
            { "uri": "file:///work/app/src/main.rs", "range": {} },
            { "targetUri": "file:///home/me/.rustup/toolchains/stable/lib/rustlib/src/rust/library/core/src/option.rs" },
            { "changes": { "file:///work/app/a.rs": [{ "newText": "x" }] } },
            { "uri": "jdt://contents/rt.jar/java.lang/String.class" }
        ]);
        mapper.map(&mut answer);
        assert_eq!(answer[0]["uri"], "src/main.rs");
        assert_eq!(answer[1]["targetUri"], "external:t1");
        assert_eq!(answer[2]["changes"]["a.rs"][0]["newText"], "x");
        assert_eq!(answer[3]["uri"], "external:t2");
        assert_eq!(mapper.externals["t1"].label, "rust core");
        assert_eq!(mapper.externals["t1"].name, "option.rs");
        assert_eq!(mapper.externals["t2"].name, "String.class");
    }

    #[test]
    fn external_files_say_where_they_come_from() {
        let label = |path: &str| describe(Path::new(path)).label;
        assert_eq!(
            label("/w/node_modules/@types/react/index.d.ts"),
            "node_modules/@types/react"
        );
        assert_eq!(
            label(
                "/home/me/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde-1.0.228/src/lib.rs"
            ),
            "crate serde-1.0.228"
        );
        assert_eq!(
            label("/usr/lib/python3.12/site-packages/requests/api.py"),
            "python requests"
        );
    }
}
