//! Clients: the local credential, listing, renaming, revoking, tokens, and
//! what the list says about each.

use std::time::Duration;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use time::Duration as TimeDuration;

use crate::support::{TestApp, TestServer, request_from as from};

/// Pairs a command line through a code: its credential and ID.
async fn pair_cli(app: &TestApp, name: &str) -> (String, String) {
    let code = app.post("/api/v1/pairings", json!({})).await.body["code"].clone();
    let paired = app
        .raw(from(
            "192.168.1.20",
            Method::POST,
            "/api/v1/pair",
            &[],
            Some(json!({ "code": code, "kind": "cli", "name": name })),
        ))
        .await;
    assert_eq!(paired.status, StatusCode::CREATED, "{}", paired.body);
    (
        paired.body["credential"].as_str().unwrap().to_owned(),
        paired.body["client"]["id"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
async fn the_local_credential_is_kept_until_it_is_revoked_or_lost() {
    let app = TestApp::start().await;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&app.credential_file)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let me = app.get("/api/v1/clients/current").await;
    assert_eq!(me.body["name"], "This computer");
    assert_eq!(me.body["kind"], "local");
    assert_eq!(me.body["pairedVia"], "local");

    // A restart keeps it.
    let kept = app
        .state
        .clients
        .ensure_local(&app.credential_file)
        .await
        .unwrap();
    assert_eq!(kept.id.to_string(), me.body["id"].as_str().unwrap());

    // Lost: a new one, and the old one stops working.
    std::fs::remove_file(&app.credential_file).unwrap();
    app.state
        .clients
        .ensure_local(&app.credential_file)
        .await
        .unwrap();
    let new = std::fs::read_to_string(&app.credential_file).unwrap();
    assert_ne!(new.trim(), app.credential);
    let old = app.get("/api/v1/clients/current").await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
    assert_eq!(old.body["code"], "not_paired");

    // Revoked: written anew at once, so the owner is never locked out.
    let id = app
        .as_client(new.trim(), Method::GET, "/api/v1/clients/current", None)
        .await
        .body["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let revoked = app
        .as_client(
            new.trim(),
            Method::DELETE,
            &format!("/api/v1/clients/{id}"),
            None,
        )
        .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    let newer = std::fs::read_to_string(&app.credential_file).unwrap();
    assert_ne!(newer, new);
    let works = app
        .as_client(newer.trim(), Method::GET, "/api/v1/clients", None)
        .await;
    assert_eq!(works.status, StatusCode::OK);
    let locals: Vec<&Value> = works.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|client| client["kind"] == "local")
        .collect();
    assert_eq!(locals.len(), 1);
}

#[tokio::test]
async fn clients_are_listed_renamed_and_revoked() {
    let app = TestApp::start().await;
    let (credential, id) = pair_cli(&app, "ahab CLI on box").await;

    let list = app.get("/api/v1/clients").await;
    let items = list.body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["current"], true);
    assert_eq!(items[1]["name"], "ahab CLI on box");
    assert_eq!(items[1]["lastAddress"], "192.168.1.20");
    assert_eq!(items[1]["encrypted"], false, "plain HTTP on a home network");

    let renamed = app
        .patch(
            &format!("/api/v1/clients/{id}"),
            json!({ "name": "Build box" }),
        )
        .await;
    assert_eq!(renamed.body["name"], "Build box");
    let invalid = app
        .patch(&format!("/api/v1/clients/{id}"), json!({ "name": " " }))
        .await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);

    let revoked = app.delete(&format!("/api/v1/clients/{id}")).await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    let refused = app
        .as_client(&credential, Method::GET, "/api/v1/clients", None)
        .await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
    assert_eq!(refused.body["code"], "not_paired");
    assert_eq!(
        app.get("/api/v1/clients").await.body["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let twice = app.delete(&format!("/api/v1/clients/{id}")).await;
    assert_eq!(twice.status, StatusCode::NOT_FOUND);
    assert_eq!(twice.body["code"], "client_not_found");
}

#[tokio::test]
async fn a_browser_that_revokes_itself_forgets_its_cookie() {
    let app = TestApp::start().await;
    let (credential, id) = pair_cli(&app, "me").await;
    let revoked = app
        .as_client(
            &credential,
            Method::DELETE,
            &format!("/api/v1/clients/{id}"),
            None,
        )
        .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    let cookie = revoked.headers["set-cookie"].to_str().unwrap();
    assert!(cookie.contains("Max-Age=0"), "{cookie}");
}

#[tokio::test]
async fn a_token_for_a_script_is_shown_once_and_works() {
    let app = TestApp::start().await;
    let created = app
        .post("/api/v1/clients/tokens", json!({ "name": "CI deploys" }))
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let token = created.body["token"].as_str().unwrap();
    assert!(token.starts_with("ahab_"));
    assert_eq!(created.body["client"]["kind"], "token");
    assert_eq!(created.body["client"]["pairedVia"], "token");

    let used = app
        .as_client(token, Method::GET, "/api/v1/machines", None)
        .await;
    assert_eq!(used.status, StatusCode::OK);
    let listed = app.get("/api/v1/clients").await;
    let text = listed.body.to_string();
    assert!(text.contains("CI deploys"));
    assert!(!text.contains(token), "never listed");
}

#[tokio::test]
async fn last_use_is_written_at_most_once_a_minute() {
    let app = TestApp::start().await;
    let (credential, id) = pair_cli(&app, "box").await;
    let last_used = || async {
        app.get("/api/v1/clients").await.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|client| client["id"] == id.as_str())
            .unwrap()["lastUsedAt"]
            .clone()
    };
    let paired_at = last_used().await;

    app.clock.advance(TimeDuration::seconds(30));
    app.as_client(&credential, Method::GET, "/api/v1/machines", None)
        .await;
    assert_eq!(last_used().await, paired_at);

    app.clock.advance(TimeDuration::seconds(31));
    app.raw(from(
        "100.64.0.7",
        Method::GET,
        "/api/v1/machines",
        &[("authorization", &format!("Bearer {credential}"))],
        None,
    ))
    .await;
    let clients = app.get("/api/v1/clients").await.body;
    let client = clients["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["id"] == id.as_str())
        .unwrap()
        .clone();
    assert_ne!(client["lastUsedAt"], paired_at);
    assert_eq!(client["lastAddress"], "100.64.0.7");
    assert_eq!(client["encrypted"], true, "a tailnet is encrypted");
}

#[tokio::test]
async fn clients_unused_for_90_days_are_marked_not_revoked() {
    let app = TestApp::start().await;
    let (credential, id) = pair_cli(&app, "old phone").await;
    app.clock.advance(TimeDuration::days(91));
    let list = app.get("/api/v1/clients").await;
    let items = list.body["items"].as_array().unwrap();
    let old = items.iter().find(|d| d["id"] == id.as_str()).unwrap();
    assert_eq!(old["unused"], true);
    let local = items.iter().find(|d| d["kind"] == "local").unwrap();
    assert_eq!(local["unused"], false);
    // Still works.
    let used = app
        .as_client(&credential, Method::GET, "/api/v1/clients", None)
        .await;
    assert_eq!(used.status, StatusCode::OK);
}

#[tokio::test]
async fn revoking_a_client_closes_its_open_streams() {
    let server = TestServer::start().await;
    let created = server
        .post("/clients/tokens", json!({ "name": "watcher" }))
        .await
        .1;
    let token = created["token"].as_str().unwrap().to_owned();
    let id = created["client"]["id"].as_str().unwrap().to_owned();
    let mut stream = reqwest::Client::new()
        .get(format!("{}/api/v1/plugin-events", server.url))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(stream.status(), StatusCode::OK);

    // Another client's stream stays open.
    let mut other = server
        .http()
        .get(format!("{}/api/v1/plugin-events", server.url))
        .send()
        .await
        .unwrap();

    let (status, _) = server
        .call(reqwest::Method::DELETE, &format!("/clients/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        while let Ok(Some(_)) = stream.chunk().await {}
    })
    .await;
    assert!(ended.is_ok(), "the revoked client's stream is closed");
    // It hears the revocation, and stays open.
    let still_open = tokio::time::timeout(Duration::from_millis(300), async {
        while let Ok(Some(_)) = other.chunk().await {}
    })
    .await;
    assert!(still_open.is_err(), "the other stream stays open");

    let refused = reqwest::Client::new()
        .get(format!("{}/api/v1/plugin-events", server.url))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn requests_without_a_credential_are_not_paired() {
    let app = TestApp::start().await;
    for (method, uri) in [
        (Method::GET, "/api/v1/machines"),
        (Method::GET, "/api/v1/clients"),
        (Method::POST, "/api/v1/projects"),
        (Method::GET, "/api/v1/plugin-events"),
        (Method::GET, "/api/v1/openapi.json"),
    ] {
        let response = app.anonymous(method, uri, Some(json!({}))).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{uri}");
        assert_eq!(response.body["code"], "not_paired", "{uri}");
    }
    let wrong = app
        .as_client("ahab_nonsense", Method::GET, "/api/v1/machines", None)
        .await;
    assert_eq!(wrong.body["code"], "not_paired");
}
