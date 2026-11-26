//! Pairing new clients: links and codes offered by a paired client, the
//! first-run link, requests a paired client answers, and the limits on
//! guessing.

use std::time::Duration;

use axum::http::{Method, StatusCode, header};
use serde_json::{Value, json};
use time::Duration as TimeDuration;

use crate::support::{FIREFOX, TestApp, request_from as from};

/// Offers a link and code as this computer: (id, secret, code).
async fn offer(app: &TestApp) -> (String, String, String) {
    let created = app.post("/api/v1/pairings", json!({})).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    (
        created.body["id"].as_str().unwrap().to_owned(),
        created.body["secret"].as_str().unwrap().to_owned(),
        created.body["code"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
async fn a_browser_pairs_once_with_a_link_and_gets_a_cookie() {
    let app = TestApp::start().await;
    let (id, secret, code) = offer(&app).await;
    assert_eq!(code.len(), 9, "{code}");
    assert_eq!(&code[4..5], "-");
    let link = app.post("/api/v1/pairings", json!({})).await;
    assert!(
        link.body["url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/pair#{}", link.body["secret"].as_str().unwrap()))
    );

    let paired = app
        .raw(from(
            "192.168.1.20",
            Method::POST,
            "/api/v1/pair",
            &[("user-agent", FIREFOX), ("host", "192.168.1.4:8180")],
            Some(json!({ "secret": secret })),
        ))
        .await;
    assert_eq!(paired.status, StatusCode::CREATED, "{}", paired.body);
    assert_eq!(paired.body["client"]["name"], "Firefox on Linux");
    assert_eq!(paired.body["client"]["kind"], "browser");
    assert_eq!(paired.body["client"]["pairedVia"], "link");
    assert_eq!(paired.body["client"]["encrypted"], false);
    // Browsers never see their credential: it is an HttpOnly cookie.
    assert!(paired.body.get("credential").is_none());
    let cookie = paired.headers[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.starts_with("ahab_device=ahab_"), "{cookie}");
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(!cookie.contains("Secure"), "plain HTTP: {cookie}");
    let pair = cookie.split(';').next().unwrap();

    // The cookie is the credential.
    let current = app
        .raw(from(
            "192.168.1.20",
            Method::GET,
            "/api/v1/clients/current",
            &[("cookie", pair)],
            None,
        ))
        .await;
    assert_eq!(current.status, StatusCode::OK, "{}", current.body);
    assert_eq!(current.body["current"], true);
    let local = app.get("/api/v1/clients/current").await.body["id"].clone();
    assert_eq!(current.body["pairedBy"], local);

    // Once only.
    let again = app
        .anonymous(
            Method::POST,
            "/api/v1/pair",
            Some(json!({ "secret": secret })),
        )
        .await;
    assert_eq!(again.status, StatusCode::FORBIDDEN);
    assert_eq!(again.body["code"], "pairing_expired");
    let state = app.get(&format!("/api/v1/pairings/{id}")).await;
    assert_eq!(state.body["state"], "used");
    assert_eq!(state.body["clientId"], current.body["id"]);
}

#[tokio::test]
async fn the_command_line_pairs_with_a_code_and_gets_its_credential() {
    let app = TestApp::start().await;
    let (_, _, code) = offer(&app).await;

    let wrong = app
        .anonymous(
            Method::POST,
            "/api/v1/pair",
            Some(json!({ "code": "ZZZZ-ZZZZ" })),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    assert_eq!(wrong.body["code"], "pairing_invalid");

    let paired = app
        .anonymous(
            Method::POST,
            "/api/v1/pair",
            Some(json!({ "code": code.to_lowercase(), "kind": "cli", "name": "ahab CLI on box" })),
        )
        .await;
    assert_eq!(paired.status, StatusCode::CREATED, "{}", paired.body);
    assert_eq!(paired.body["client"]["pairedVia"], "code");
    assert!(paired.headers.get(header::SET_COOKIE).is_none());
    let credential = paired.body["credential"].as_str().unwrap();
    let me = app
        .as_client(credential, Method::GET, "/api/v1/clients/current", None)
        .await;
    assert_eq!(me.body["name"], "ahab CLI on box");
    assert_eq!(me.body["kind"], "cli");
}

#[tokio::test]
async fn links_and_codes_expire_after_ten_minutes() {
    let app = TestApp::start().await;
    let (_, secret, code) = offer(&app).await;
    app.clock.advance(TimeDuration::minutes(11));
    let by_link = app
        .anonymous(
            Method::POST,
            "/api/v1/pair",
            Some(json!({ "secret": secret })),
        )
        .await;
    assert_eq!(by_link.body["code"], "pairing_expired");
    let by_code = app
        .anonymous(Method::POST, "/api/v1/pair", Some(json!({ "code": code })))
        .await;
    assert_eq!(by_code.body["code"], "pairing_invalid");
}

#[tokio::test]
async fn a_withdrawn_link_no_longer_works() {
    let app = TestApp::start().await;
    let (id, secret, _) = offer(&app).await;
    let withdrawn = app.delete(&format!("/api/v1/pairings/{id}")).await;
    assert_eq!(withdrawn.status, StatusCode::NO_CONTENT);
    let used = app
        .anonymous(
            Method::POST,
            "/api/v1/pair",
            Some(json!({ "secret": secret })),
        )
        .await;
    assert_eq!(used.status, StatusCode::FORBIDDEN);
}

async fn ask(app: &TestApp, peer: &str, body: Value) -> Value {
    let asked = app
        .raw(from(
            peer,
            Method::POST,
            "/api/v1/pairing-requests",
            &[("user-agent", "ahab/0.1.0")],
            Some(body),
        ))
        .await;
    assert_eq!(asked.status, StatusCode::CREATED, "{}", asked.body);
    asked.body
}

#[tokio::test]
async fn a_new_client_asks_and_a_paired_one_approves() {
    let app = TestApp::start().await;
    let mut events = app.state.presence.bus().subscribe();
    let asked = ask(
        &app,
        "192.168.1.30",
        json!({ "name": "ahab CLI on build-box", "kind": "cli" }),
    )
    .await;
    let code = asked["code"].as_str().unwrap();
    assert_eq!(code.len(), 4);
    let id = asked["id"].as_str().unwrap();
    let secret = asked["secret"].as_str().unwrap();

    // Paired browsers hear of it, and can list it.
    let event = events.recv().await.unwrap();
    assert_eq!(event.name, "pairing.requested");
    assert_eq!(event.data["code"], code);
    assert_eq!(event.data["address"], "192.168.1.30");
    let open = app.get("/api/v1/pairing-requests").await;
    assert_eq!(open.body["items"][0]["clientName"], "ahab CLI on build-box");
    assert_eq!(open.body["items"][0]["address"], "192.168.1.30");
    // Asking for the list needs a credential.
    let anonymous = app
        .anonymous(Method::GET, "/api/v1/pairing-requests", None)
        .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

    // Nothing yet.
    let wait = format!("/api/v1/pairing-requests/{id}?secret={secret}&wait=0");
    let waiting = app.anonymous(Method::GET, &wait, None).await;
    assert_eq!(waiting.body["state"], "open");
    assert!(waiting.body.get("credential").is_none());
    let wrong = app
        .anonymous(
            Method::GET,
            &format!("/api/v1/pairing-requests/{id}?secret=nope&wait=0"),
            None,
        )
        .await;
    assert_eq!(wrong.body["code"], "pairing_invalid");

    let approved = app
        .post(
            &format!("/api/v1/pairing-requests/{}/approve", code.to_lowercase()),
            json!({}),
        )
        .await;
    assert_eq!(approved.status, StatusCode::OK, "{}", approved.body);
    assert_eq!(approved.body["state"], "approved");

    let answer = app.anonymous(Method::GET, &wait, None).await;
    assert_eq!(answer.body["state"], "used", "{}", answer.body);
    let credential = answer.body["credential"].as_str().unwrap();
    let me = app
        .as_client(credential, Method::GET, "/api/v1/clients/current", None)
        .await;
    assert_eq!(me.body["pairedVia"], "request");
    assert_eq!(me.body["encrypted"], true, "it waited from this computer");
    assert_eq!(me.body["name"], "ahab CLI on build-box");
    let local = app.get("/api/v1/clients/current").await.body["id"].clone();
    assert_eq!(me.body["pairedBy"], local);

    // The credential is handed over once.
    let again = app.anonymous(Method::GET, &wait, None).await;
    assert_eq!(again.body["code"], "pairing_expired");
    assert!(
        app.get("/api/v1/pairing-requests").await.body["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn the_wait_ends_as_soon_as_the_request_is_answered() {
    let app = std::sync::Arc::new(TestApp::start().await);
    let asked = ask(&app, "192.168.1.30", json!({ "kind": "browser" })).await;
    let id = asked["id"].as_str().unwrap().to_owned();
    let secret = asked["secret"].as_str().unwrap().to_owned();
    let waiter = {
        let app = std::sync::Arc::clone(&app);
        tokio::spawn(async move {
            app.raw(from(
                "192.168.1.30",
                Method::GET,
                &format!("/api/v1/pairing-requests/{id}?secret={secret}&wait=20"),
                &[],
                None,
            ))
            .await
        })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    let started = std::time::Instant::now();
    app.post(
        &format!(
            "/api/v1/pairing-requests/{}/approve",
            asked["id"].as_str().unwrap()
        ),
        json!({}),
    )
    .await;
    let answer = waiter.await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(answer.body["state"], "used");
    // A browser gets a cookie, not the credential.
    assert!(answer.body.get("credential").is_none());
    assert!(
        answer.headers[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .starts_with("ahab_device=")
    );
}

#[tokio::test]
async fn a_denied_or_expired_request_pairs_nothing() {
    let app = TestApp::start().await;
    let asked = ask(&app, "192.168.1.30", json!({})).await;
    let wait = format!(
        "/api/v1/pairing-requests/{}?secret={}&wait=0",
        asked["id"].as_str().unwrap(),
        asked["secret"].as_str().unwrap()
    );
    let denied = app
        .post(
            &format!(
                "/api/v1/pairing-requests/{}/deny",
                asked["code"].as_str().unwrap()
            ),
            json!({}),
        )
        .await;
    assert_eq!(denied.body["state"], "denied");
    assert_eq!(
        app.anonymous(Method::GET, &wait, None).await.body["state"],
        "denied"
    );
    // Answered already.
    let late = app
        .post(
            &format!(
                "/api/v1/pairing-requests/{}/approve",
                asked["code"].as_str().unwrap()
            ),
            json!({}),
        )
        .await;
    assert_eq!(late.status, StatusCode::NOT_FOUND);
    assert_eq!(late.body["code"], "pairing_not_found");

    let expiring = ask(&app, "192.168.1.31", json!({})).await;
    app.clock.advance(TimeDuration::minutes(11));
    let gone = app
        .anonymous(
            Method::GET,
            &format!(
                "/api/v1/pairing-requests/{}?secret={}&wait=0",
                expiring["id"].as_str().unwrap(),
                expiring["secret"].as_str().unwrap()
            ),
            None,
        )
        .await;
    assert_eq!(gone.body["code"], "pairing_expired");
    let approve = app
        .post(
            &format!(
                "/api/v1/pairing-requests/{}/approve",
                expiring["code"].as_str().unwrap()
            ),
            json!({}),
        )
        .await;
    assert_eq!(approve.status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.get("/api/v1/clients").await.body["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn at_most_twenty_requests_wait_at_once() {
    let app = TestApp::start().await;
    for n in 0..20 {
        ask(&app, &format!("10.0.{n}.1"), json!({})).await;
    }
    let one_more = app
        .raw(from(
            "10.0.99.1",
            Method::POST,
            "/api/v1/pairing-requests",
            &[],
            Some(json!({})),
        ))
        .await;
    assert_eq!(one_more.status, StatusCode::CONFLICT);
    assert_eq!(one_more.body["code"], "too_many_requests");
}

#[tokio::test]
async fn ten_tries_a_minute_per_address_then_a_growing_wait() {
    let app = TestApp::start().await;
    let try_code = |peer: &'static str| {
        from(
            peer,
            Method::POST,
            "/api/v1/pair",
            &[],
            Some(json!({ "code": "ZZZZ-ZZZZ" })),
        )
    };
    for _ in 0..10 {
        let tried = app.raw(try_code("192.168.1.66")).await;
        assert_eq!(tried.body["code"], "pairing_invalid");
    }
    let blocked = app.raw(try_code("192.168.1.66")).await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(blocked.body["code"], "too_many_attempts");
    assert_eq!(blocked.headers[header::RETRY_AFTER], "60");
    // Even the right secret waits.
    let (_, secret, _) = offer(&app).await;
    let right = app
        .raw(from(
            "192.168.1.66",
            Method::POST,
            "/api/v1/pair",
            &[],
            Some(json!({ "secret": secret })),
        ))
        .await;
    assert_eq!(right.status, StatusCode::TOO_MANY_REQUESTS);
    // Another address does not.
    assert_eq!(
        app.raw(try_code("192.168.1.67")).await.body["code"],
        "pairing_invalid"
    );

    // After the wait, ten more; then twice as long.
    app.clock.advance(TimeDuration::minutes(1));
    for _ in 0..10 {
        assert_eq!(
            app.raw(try_code("192.168.1.66")).await.status,
            StatusCode::FORBIDDEN
        );
    }
    let longer = app.raw(try_code("192.168.1.66")).await;
    assert_eq!(longer.headers[header::RETRY_AFTER], "120");
}

#[tokio::test]
async fn fifty_wrong_codes_in_an_hour_withdraw_every_open_code() {
    let app = TestApp::start().await;
    let (_, _, code) = offer(&app).await;
    let (_, secret, _) = offer(&app).await;
    for n in 0..50 {
        let peer = format!("10.1.{}.1", n / 10);
        let tried = app
            .raw(from(
                &peer,
                Method::POST,
                "/api/v1/pair",
                &[],
                Some(json!({ "code": "ZZZZ-ZZZZ" })),
            ))
            .await;
        assert_eq!(tried.body["code"], "pairing_invalid", "try {n}");
    }
    let right = app
        .raw(from(
            "10.9.9.9",
            Method::POST,
            "/api/v1/pair",
            &[],
            Some(json!({ "code": code })),
        ))
        .await;
    assert_eq!(right.status, StatusCode::FORBIDDEN, "withdrawn");
    let link = app
        .anonymous(
            Method::POST,
            "/api/v1/pair",
            Some(json!({ "secret": secret })),
        )
        .await;
    assert_eq!(link.status, StatusCode::FORBIDDEN, "its link too");
}

#[tokio::test]
async fn the_first_run_link_works_once_for_a_day_until_a_client_is_paired() {
    let app = TestApp::start().await;
    let first = app.state.pairings.first_run().await.unwrap().unwrap();
    assert!(first.code.is_none());
    // A new start prints a new one; the earlier stops working.
    let second = app.state.pairings.first_run().await.unwrap().unwrap();
    let earlier = app
        .anonymous(
            Method::POST,
            "/api/v1/pair",
            Some(json!({ "secret": first.secret })),
        )
        .await;
    assert_eq!(earlier.body["code"], "pairing_expired");

    app.clock.advance(TimeDuration::hours(23));
    let paired = app
        .anonymous(
            Method::POST,
            "/api/v1/pair",
            Some(json!({ "secret": second.secret })),
        )
        .await;
    assert_eq!(paired.status, StatusCode::CREATED, "{}", paired.body);
    // Printed by the server: nobody paired it.
    assert_eq!(paired.body["client"]["pairedBy"], Value::Null);
    assert!(app.state.pairings.first_run().await.unwrap().is_none());

    let late = TestApp::start().await;
    let expired = late.state.pairings.first_run().await.unwrap().unwrap();
    late.clock.advance(TimeDuration::hours(25));
    let used = late
        .anonymous(
            Method::POST,
            "/api/v1/pair",
            Some(json!({ "secret": expired.secret })),
        )
        .await;
    assert_eq!(used.body["code"], "pairing_expired");
}

#[tokio::test]
async fn pairing_any_client_closes_the_first_run_link() {
    let app = TestApp::start().await;
    let first = app.state.pairings.first_run().await.unwrap().unwrap();
    let asked = ask(&app, "192.168.1.30", json!({ "kind": "cli" })).await;
    app.post(
        &format!(
            "/api/v1/pairing-requests/{}/approve",
            asked["code"].as_str().unwrap()
        ),
        json!({}),
    )
    .await;
    app.anonymous(
        Method::GET,
        &format!(
            "/api/v1/pairing-requests/{}?secret={}&wait=0",
            asked["id"].as_str().unwrap(),
            asked["secret"].as_str().unwrap()
        ),
        None,
    )
    .await;
    let closed = app
        .anonymous(
            Method::POST,
            "/api/v1/pair",
            Some(json!({ "secret": first.secret })),
        )
        .await;
    assert_eq!(closed.body["code"], "pairing_expired");
}

#[tokio::test]
async fn only_this_computer_asks_for_the_first_run_link_and_only_while_unpaired() {
    let app = TestApp::start().await;
    let first = app
        .post("/api/v1/pairings", json!({ "firstRun": true }))
        .await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    assert_eq!(first.body["code"], Value::Null);
    let paired = app
        .anonymous(
            Method::POST,
            "/api/v1/pair",
            Some(json!({ "secret": first.body["secret"], "kind": "cli" })),
        )
        .await;
    let credential = paired.body["credential"].as_str().unwrap();
    let not_local = app
        .as_client(
            credential,
            Method::POST,
            "/api/v1/pairings",
            Some(json!({ "firstRun": true })),
        )
        .await;
    assert_eq!(not_local.body["code"], "not_local");
    let paired_already = app
        .post("/api/v1/pairings", json!({ "firstRun": true }))
        .await;
    assert_eq!(paired_already.body["code"], "already_paired");
}
