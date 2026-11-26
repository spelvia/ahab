//! Session folders in folders, pinned folders, and deleting one: what it
//! held moves up to where it was.

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::support::TestServer;

async fn folder(server: &TestServer, name: &str, parent: Option<&str>) -> String {
    let (status, made) = server
        .post(
            "/projects/app/session-folders",
            json!({ "name": name, "parentId": parent }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    made["id"].as_str().unwrap().to_owned()
}

async fn folders(server: &TestServer) -> Vec<Value> {
    server.get("/projects/app/session-folders").await.1["items"]
        .as_array()
        .unwrap()
        .clone()
}

fn find<'a>(all: &'a [Value], id: &str) -> &'a Value {
    all.iter().find(|folder| folder["id"] == id).unwrap()
}

#[tokio::test]
async fn folders_nest_move_pin_and_hand_their_contents_up_when_deleted() {
    let server = TestServer::start().await;
    server.stored_project("app").await;
    let work = folder(&server, "Work", None).await;
    let bugs = folder(&server, "Bugs", Some(&work)).await;
    let deep = folder(&server, "Flaky", Some(&bugs)).await;

    let all = folders(&server).await;
    assert!(find(&all, &work)["parentId"].is_null());
    assert_eq!(find(&all, &bugs)["parentId"], work.as_str());
    assert_eq!(find(&all, &deep)["parentId"], bugs.as_str());

    // Never into itself, nor into one of its own folders.
    for into in [&work, &deep] {
        let (status, refused) = server
            .call(
                Method::PATCH,
                &format!("/session-folders/{work}"),
                Some(json!({ "parentId": into })),
            )
            .await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert_eq!(refused["code"], "folder_in_itself");
    }

    // Moved to the top level, renamed and pinned at once.
    let (status, moved) = server
        .call(
            Method::PATCH,
            &format!("/session-folders/{deep}"),
            Some(json!({ "parentId": null, "name": "Flaky tests", "pinned": true })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert!(moved["parentId"].is_null());
    assert_eq!(moved["name"], "Flaky tests");
    assert!(moved["pinnedAt"].is_string());
    // Leaving it out keeps it: only the name changes.
    let (_, kept) = server
        .call(
            Method::PATCH,
            &format!("/session-folders/{deep}"),
            Some(json!({ "name": "Flaky" })),
        )
        .await;
    assert!(kept["parentId"].is_null());
    assert!(kept["pinnedAt"].is_string());
    let (_, unpinned) = server
        .call(
            Method::PATCH,
            &format!("/session-folders/{deep}"),
            Some(json!({ "pinned": false })),
        )
        .await;
    assert!(unpinned["pinnedAt"].is_null());

    // Deleting Work: Bugs moves up to the top level.
    let (status, _) = server
        .call(Method::DELETE, &format!("/session-folders/{work}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let all = folders(&server).await;
    assert_eq!(all.len(), 2);
    assert!(find(&all, &bugs)["parentId"].is_null());

    // A folder of another project is no parent.
    server.stored_project("other").await;
    let (status, _) = server
        .post(
            "/projects/other/session-folders",
            json!({ "name": "Elsewhere", "parentId": bugs }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
