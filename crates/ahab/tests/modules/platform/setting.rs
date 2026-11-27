//! Ahab's own settings through the API (the editor's): defaults,
//! the server's values, a project's over them, and what is refused.

use reqwest::{Method, StatusCode};
use serde_json::json;

use crate::support::TestServer;

#[tokio::test]
async fn editor_settings_are_set_for_the_server_and_per_project() {
    let server = TestServer::start().await;

    let (status, editor) = server.get("/settings/editor").await;
    assert_eq!(status, StatusCode::OK, "{editor}");
    assert_eq!(editor["values"]["editableByDefault"], false);
    assert_eq!(editor["values"]["fontSize"], 13);
    assert_eq!(editor["values"]["autoSave"], "off");
    assert_eq!(editor["server"], json!({}));
    assert_eq!(editor["project"], json!(null));
    let vim = editor["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["name"] == "vim")
        .unwrap();
    assert_eq!(vim["kind"], "boolean");
    assert_eq!(vim["perProject"], false);

    let (status, changed) = server
        .call(
            Method::PATCH,
            "/settings/editor",
            Some(json!({ "fontSize": 15, "editableByDefault": true, "vim": true })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["values"]["fontSize"], 15);

    // A project makes its files open read-only again.
    let project = uuid::Uuid::now_v7();
    let (status, own) = server
        .call(
            Method::PATCH,
            &format!("/settings/editor?project={project}"),
            Some(json!({ "editableByDefault": false })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{own}");
    assert_eq!(own["values"]["editableByDefault"], false);
    assert_eq!(own["values"]["fontSize"], 15);
    assert_eq!(own["project"], json!({ "editableByDefault": false }));
    let (_, other) = server.get("/settings/editor").await;
    assert_eq!(other["values"]["editableByDefault"], true);

    // Null unsets; what does not fit is refused.
    let (_, unset) = server
        .call(
            Method::PATCH,
            "/settings/editor",
            Some(json!({ "fontSize": null })),
        )
        .await;
    assert_eq!(unset["values"]["fontSize"], 13);
    assert_eq!(
        unset["server"],
        json!({ "editableByDefault": true, "vim": true })
    );
    for bad in [
        json!({ "fontSize": 300 }),
        json!({ "autoSave": "always" }),
        json!({ "unknown": true }),
    ] {
        let (status, refused) = server
            .call(Method::PATCH, "/settings/editor", Some(bad))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    }
    let (status, _) = server
        .call(
            Method::PATCH,
            &format!("/settings/editor?project={project}"),
            Some(json!({ "vim": false })),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "vim is per person"
    );
    let (status, missing) = server.get("/settings/nothing").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "settings_not_found");
}
