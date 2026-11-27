//! `ahab://` links and pasted pairing links. Parsing only: a link
//! never pairs, joins or opens anything by itself. The built-in screens show
//! what a link asks for (the server, its fingerprint) and act only when the
//! person confirms.
//!
//! - `ahab://pair?server=<url>&secret=<secret>` or `…&code=<code>`
//! - `ahab://join?server=<url>&code=<code>[&fingerprint=SHA256:…]`
//! - `ahab://open?server=<url>[&path=/p/…]`
//!
//! A pairing link the web app shows (`https://ahab.example/pair#<secret>`)
//! is accepted too, pasted into the connect screen.

use serde::Serialize;
use url::Url;

/// What a link asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Link {
    /// Pair this app with `server`, with a link's secret or a code.
    Pair {
        server: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        secret: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    /// Join this computer to `server` as a machine (`ahab setup`).
    Join {
        server: String,
        code: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        fingerprint: Option<String>,
    },
    /// Open a page of `server`'s web app.
    Open { server: String, path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error("This is not an Ahab link.")]
    NotALink,
    #[error("Ahab links pair, join or open; `{0}` is none of them.")]
    UnknownAction(String),
    #[error("The link has no server address, or not an http(s) one.")]
    BadServer,
    #[error("The link is missing its {0}.")]
    Missing(&'static str),
    #[error("The link's page is not a page of the server.")]
    BadPath,
}

/// A server address as the app keeps it: scheme, host and port, no path
/// and no trailing slash. Without a scheme, https.
pub fn normalize_server(input: &str) -> Option<String> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    let with_scheme = if input.contains("://") {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    let url = Url::parse(&with_scheme).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    Some(url.origin().ascii_serialization())
}

/// Reads a link: `ahab://…`, or an `https://…/pair#<secret>` pairing link.
pub fn parse(input: &str) -> Result<Link, LinkError> {
    let url = Url::parse(input.trim()).map_err(|_| LinkError::NotALink)?;
    match url.scheme() {
        "ahab" => parse_ahab(&url),
        "http" | "https" => parse_web_pairing(&url),
        _ => Err(LinkError::NotALink),
    }
}

fn parse_ahab(url: &Url) -> Result<Link, LinkError> {
    // `ahab://pair?…`: the action is the host; `ahab:pair?…` its path.
    let action = url
        .host_str()
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| url.path().trim_matches('/'))
        .to_ascii_lowercase();
    let query = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let server = query("server")
        .as_deref()
        .and_then(normalize_server)
        .ok_or(LinkError::BadServer)?;
    match action.as_str() {
        "pair" => {
            let secret = query("secret");
            let code = query("code");
            if secret.is_none() && code.is_none() {
                return Err(LinkError::Missing("secret or code"));
            }
            Ok(Link::Pair {
                server,
                secret,
                code,
            })
        }
        "join" => Ok(Link::Join {
            server,
            code: query("code").ok_or(LinkError::Missing("code"))?,
            fingerprint: query("fingerprint"),
        }),
        "open" => {
            let path = query("path").unwrap_or_else(|| "/".to_owned());
            Ok(Link::Open {
                server,
                path: safe_path(&path).ok_or(LinkError::BadPath)?,
            })
        }
        other => Err(LinkError::UnknownAction(other.to_owned())),
    }
}

/// `https://ahab.example/pair#<secret>`, as Add client shows it.
fn parse_web_pairing(url: &Url) -> Result<Link, LinkError> {
    if url.path().trim_end_matches('/') != "/pair" {
        return Err(LinkError::NotALink);
    }
    let secret = url
        .fragment()
        .map(str::trim)
        .filter(|secret| !secret.is_empty())
        .ok_or(LinkError::Missing("secret"))?;
    let server = normalize_server(url.as_str()).ok_or(LinkError::BadServer)?;
    Ok(Link::Pair {
        server,
        secret: Some(secret.to_owned()),
        code: None,
    })
}

/// A path within the server: absolute, not another host (`//evil`), no
/// scheme, no control characters.
pub fn safe_path(path: &str) -> Option<String> {
    if !path.starts_with('/') || path.starts_with("//") || path.starts_with("/\\") {
        return None;
    }
    if path.chars().any(char::is_control) {
        return None;
    }
    let joined = Url::parse("http://x.invalid").ok()?.join(path).ok()?;
    if joined.host_str() != Some("x.invalid") {
        return None;
    }
    let mut kept = joined.path().to_owned();
    if let Some(query) = joined.query() {
        kept.push('?');
        kept.push_str(query);
    }
    if let Some(fragment) = joined.fragment() {
        kept.push('#');
        kept.push_str(fragment);
    }
    Some(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_server_addresses_to_their_origin() {
        assert_eq!(
            normalize_server("ahab.example").as_deref(),
            Some("https://ahab.example")
        );
        assert_eq!(
            normalize_server(" http://127.0.0.1:8185/ ").as_deref(),
            Some("http://127.0.0.1:8185")
        );
        assert_eq!(
            normalize_server("https://ahab.example/settings?x=1").as_deref(),
            Some("https://ahab.example")
        );
        assert_eq!(normalize_server("ftp://ahab.example"), None);
        assert_eq!(normalize_server("https://me:pw@ahab.example"), None);
        assert_eq!(normalize_server(""), None);
        assert_eq!(normalize_server("javascript:alert(1)"), None);
    }

    #[test]
    fn reads_pairing_links() {
        assert_eq!(
            parse("ahab://pair?server=https%3A%2F%2Fahab.example&secret=9fK2").unwrap(),
            Link::Pair {
                server: "https://ahab.example".into(),
                secret: Some("9fK2".into()),
                code: None,
            }
        );
        assert_eq!(
            parse("ahab://pair?server=http://192.168.1.4:8180&code=K7QM-4RXT").unwrap(),
            Link::Pair {
                server: "http://192.168.1.4:8180".into(),
                secret: None,
                code: Some("K7QM-4RXT".into()),
            }
        );
        assert_eq!(
            parse("https://ahab.example/pair#9fK2").unwrap(),
            Link::Pair {
                server: "https://ahab.example".into(),
                secret: Some("9fK2".into()),
                code: None,
            }
        );
        assert_eq!(
            parse("ahab://pair?server=https://ahab.example"),
            Err(LinkError::Missing("secret or code"))
        );
        assert_eq!(
            parse("https://ahab.example/pair"),
            Err(LinkError::Missing("secret"))
        );
        assert_eq!(
            parse("https://ahab.example/settings#x"),
            Err(LinkError::NotALink)
        );
    }

    #[test]
    fn reads_join_links_with_their_fingerprint() {
        assert_eq!(
            parse("ahab://join?server=https://atlas.lan&code=ABCD-EFGH&fingerprint=SHA256:abc")
                .unwrap(),
            Link::Join {
                server: "https://atlas.lan".into(),
                code: "ABCD-EFGH".into(),
                fingerprint: Some("SHA256:abc".into()),
            }
        );
        assert_eq!(
            parse("ahab://join?server=https://atlas.lan"),
            Err(LinkError::Missing("code"))
        );
    }

    #[test]
    fn reads_open_links_and_keeps_them_on_the_server() {
        assert_eq!(
            parse("ahab://open?server=https://atlas.lan&path=/p/abc/w/def").unwrap(),
            Link::Open {
                server: "https://atlas.lan".into(),
                path: "/p/abc/w/def".into(),
            }
        );
        assert_eq!(
            parse("ahab://open?server=https://atlas.lan").unwrap(),
            Link::Open {
                server: "https://atlas.lan".into(),
                path: "/".into(),
            }
        );
        for path in [
            "//evil.example/x",
            "https://evil.example",
            "/\\evil.example",
            "x",
        ] {
            let link = format!(
                "ahab://open?server=https://atlas.lan&path={}",
                url::form_urlencoded::byte_serialize(path.as_bytes()).collect::<String>()
            );
            assert_eq!(parse(&link), Err(LinkError::BadPath), "{path}");
        }
    }

    #[test]
    fn refuses_anything_else() {
        assert_eq!(parse("not a link"), Err(LinkError::NotALink));
        assert_eq!(parse("file:///etc/passwd"), Err(LinkError::NotALink));
        assert_eq!(
            parse("ahab://run?server=https://atlas.lan&command=rm"),
            Err(LinkError::UnknownAction("run".into()))
        );
        assert_eq!(
            parse("ahab://pair?server=file:///x&secret=s"),
            Err(LinkError::BadServer)
        );
        assert_eq!(parse("ahab://pair?secret=s"), Err(LinkError::BadServer));
    }
}
