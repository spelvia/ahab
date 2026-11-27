//! The web app: from a folder, from the binary, or turned off. The API
//! answers in every case.

use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};

use ahab::server::web::WebApp;

use crate::support::TestApp;

/// A built web app in a temporary folder.
fn built_web_app() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<p>the web app</p>").unwrap();
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/app-1a2b.js"), "console.log(1)").unwrap();
    dir
}

#[tokio::test]
async fn serves_a_folder_with_its_routes_falling_back_to_index_html() {
    let dir = built_web_app();
    let app = TestApp::with_web(WebApp::Folder(dir.path().to_owned())).await;

    let index = app.get("/").await;
    assert_eq!(index.status, StatusCode::OK);
    assert_eq!(index.text, "<p>the web app</p>");

    let route = app.get("/machines/0193").await;
    assert_eq!(route.status, StatusCode::OK);
    assert_eq!(route.text, "<p>the web app</p>");

    let script = app.get("/assets/app-1a2b.js").await;
    assert_eq!(script.status, StatusCode::OK);
    assert_eq!(script.text, "console.log(1)");

    assert_eq!(app.get("/api/v1/health").await.status, StatusCode::OK);
}

#[tokio::test]
async fn serves_the_embedded_web_app_or_says_it_was_not_built() {
    let app = TestApp::with_web(WebApp::Embedded).await;

    let index = app.get("/").await;
    assert_eq!(index.status, StatusCode::OK);
    assert!(
        index.headers[CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert_eq!(index.headers[CACHE_CONTROL], "no-cache");
    // A route of the web app gets the same page.
    assert_eq!(app.get("/machines").await.text, index.text);

    assert_eq!(app.get("/api/v1/health").await.status, StatusCode::OK);
}

#[tokio::test]
async fn turned_off_it_serves_only_the_api() {
    let app = TestApp::with_web(WebApp::Off).await;

    let root = app.get("/").await;
    assert_eq!(root.status, StatusCode::OK);
    assert!(root.text.contains("The web app is turned off"));

    assert_eq!(app.get("/machines").await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/api/v1/health").await.status, StatusCode::OK);
    assert_eq!(
        app.get("/api/v1/nothing-here").await.body["code"],
        "not_found"
    );
}

#[test]
fn chooses_where_the_web_app_comes_from() {
    let dir = std::path::Path::new("/srv/ui");
    assert_eq!(WebApp::from_settings(true, None), WebApp::Embedded);
    assert_eq!(
        WebApp::from_settings(true, Some(dir)),
        WebApp::Folder(dir.to_owned())
    );
    assert_eq!(WebApp::from_settings(false, Some(dir)), WebApp::Off);
}
