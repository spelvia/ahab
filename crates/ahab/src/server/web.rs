//! Serving the web app: from the binary (the default), from a folder
//! (development), or not at all (`--web=false`, API only).
//!
//! Release builds carry `apps/ui/dist` inside the binary; debug builds read it
//! from disk. Build the web app before a release build: a rebuilt
//! `index.html` (which names Vite's hashed files) makes Cargo embed the new
//! files. A build without `apps/ui/dist` (a Rust-only build, CI) serves a page
//! saying the web app was not built.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::Body;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use tower_http::services::{ServeDir, ServeFile};

/// The built web app, `apps/ui/dist`.
#[derive(rust_embed::RustEmbed)]
#[folder = "../../apps/ui/dist"]
#[allow_missing = true]
struct Assets;

/// Where the web app comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebApp {
    /// The web app inside the binary.
    Embedded,
    /// A built web app in a folder (`AHAB_UI_DIR`).
    Folder(PathBuf),
    /// No web app: only the API.
    Off,
}

impl WebApp {
    /// From the settings: off, a folder when given, else the embedded one.
    pub fn from_settings(web: bool, ui_dir: Option<&Path>) -> Self {
        match (web, ui_dir) {
            (false, _) => Self::Off,
            (true, Some(dir)) => Self::Folder(dir.to_owned()),
            (true, None) => Self::Embedded,
        }
    }
}

/// Adds the web app as the fallback for every path the API does not answer.
pub fn serve(router: Router, web: &WebApp) -> Router {
    match web {
        WebApp::Embedded => router.fallback(embedded),
        WebApp::Folder(dir) => router
            .fallback_service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")))),
        WebApp::Off => router.fallback(off),
    }
}

/// A file of the embedded web app, or `index.html` for the app's own routes.
async fn embedded(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if let Some(file) = Assets::get(path).filter(|_| !path.is_empty()) {
        // Vite puts content-hashed files in assets/, so they never change.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return asset(file, cache);
    }
    if let Some(index) = Assets::get("index.html") {
        return asset(index, "no-cache");
    }
    let mut response = page(
        StatusCode::OK,
        "The web app was not built",
        "This build of Ahab does not include the web app. Build it with \
         <code>pnpm --filter @ahab/ui build</code> before building Ahab, or use \
         <code>make preview</code>.",
    );
    // Not kept: once a build has the web app, the browser gets it.
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

fn asset(file: rust_embed::EmbeddedFile, cache: &'static str) -> Response {
    let mime = file.metadata.mimetype().to_owned();
    let mut response = Body::from(file.data.into_owned()).into_response();
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&mime) {
        headers.insert(CONTENT_TYPE, value);
    }
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

/// With the web app off: a short page at `/`, nothing elsewhere.
async fn off(uri: Uri) -> Response {
    let status = if uri.path() == "/" {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    page(
        status,
        "The web app is turned off",
        "This Ahab server serves only its API, at <code>/api/v1</code> \
         (documentation at <a href=\"/api/docs\">/api/docs</a>). Use the desktop or \
         mobile app, or the <code>ahab</code> command line.",
    )
}

fn page(status: StatusCode, title: &str, body: &str) -> Response {
    let html = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{title} · Ahab</title>\
         <style>body{{font:16px/1.5 system-ui,sans-serif;max-width:40rem;margin:4rem auto;\
         padding:0 1rem;color:#2b2b2b;background:#f7f6f3}}code{{font-size:.9em}}</style>\
         </head><body><h1>{title}</h1><p>{body}</p></body></html>"
    );
    (status, Html(html)).into_response()
}
