//! A plugin's problems through the diagnostics API (plugin API 0.1.3): set
//! and deleted by its daemon entry, confined to the project, shown in
//! Problems and the editor, turned off per project, asked for by Check
//! project, and cleared when the plugin is disabled.

use reqwest::StatusCode;
use serde_json::{Value, json};

use super::{local_daemon, notes_copy};
use crate::modules::tools::Window;
use crate::support::TestServer;

/// The project's problems from the notes plugin, once `wanted` holds.
async fn notes(
    server: &TestServer,
    project: &str,
    wanted: impl Fn(&[Value]) -> bool,
) -> Vec<Value> {
    let mut last = Vec::new();
    for _ in 0..200 {
        let (_, problems) = server.get(&format!("/projects/{project}/problems")).await;
        last = problems["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["source"] == "notes.notes")
            .cloned()
            .collect::<Vec<_>>();
        if wanted(&last) {
            return last;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the notes never matched; last: {last:?}");
}

#[tokio::test]
async fn a_plugins_collection_shows_in_problems_until_it_stops() {
    let server = TestServer::start().await;
    let (_machine, _daemon) = local_daemon(&server, None).await;
    let dir = tempfile::tempdir().unwrap();
    let folder = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(folder.join("README.md"), "# Title\n").unwrap();
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": folder }),
        )
        .await;
    let project_id = project["id"].as_str().unwrap().to_owned();
    let project = project["id"].as_str().unwrap().to_owned();
    let copy = notes_copy();
    let (status, installed) = server
        .post(
            "/plugins",
            json!({ "source": "folder", "location": copy.path() }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{installed}");
    server.post("/plugins/notes/enable", json!({})).await;
    let mut running = false;
    for _ in 0..200 {
        let (_, states) = server.get("/plugin-states").await;
        running = states["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["plugin"] == "notes" && s["state"] == "running");
        if running {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(running, "the notes plugin never ran");
    assert_eq!(
        server.get("/plugin-states").await.1["items"][0]["declared"]["diagnostics"],
        json!({ "check": true })
    );

    // Set: in Problems, as the plugin's collection; paths outside dropped.
    let (status, answer) = server
        .post(
            "/plugins/notes/commands/report",
            json!({ "project": project, "input": {} }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let found = notes(&server, &project, |items| !items.is_empty()).await;
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["path"], "README.md");
    assert_eq!(found[0]["severity"], "warning");
    assert_eq!(found[0]["code"], "title");
    assert_eq!(found[0]["line"], 1);
    assert_eq!(found[0]["column"], 3);
    // Its fix is kept with it, for the editor to offer.
    assert_eq!(found[0]["fixes"][0]["title"], "Call it “Heading”");
    assert_eq!(found[0]["fixes"][0]["edits"][0]["newText"], "Heading");

    // In the editor too, as every source's problems.
    let mut window = Window::open(&server, &project).await;
    window
        .send(json!({ "type": "open", "path": "README.md", "language": "markdown", "version": 1, "text": "# Title\n" }))
        .await;
    let frame = window
        .until(|f| f["type"] == "diagnostics" && f["source"] == "notes.notes")
        .await;
    assert_eq!(frame["items"][0]["message"], "A note about the title");
    assert_eq!(
        frame["items"][0]["data"]["fixes"][0]["edits"][0]["range"]["end"]["character"],
        7
    );

    // Turned off for the project: hidden, as a tool would be.
    server
        .put(
            &format!("/projects/{project_id}/tool-settings"),
            json!({ "tools": { "notes.notes": { "off": true } } }),
        )
        .await;
    notes(&server, &project, <[Value]>::is_empty).await;
    server
        .put(&format!("/projects/{project_id}/tool-settings"), json!({}))
        .await;
    notes(&server, &project, |items| items.len() == 1).await;

    // Deleted by the plugin.
    server
        .post(
            "/plugins/notes/commands/forget",
            json!({ "project": project, "input": {} }),
        )
        .await;
    notes(&server, &project, <[Value]>::is_empty).await;

    // Check project asks it, for the project's own folder here.
    let (status, ran) = server
        .post(&format!("/projects/{project}/checks"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{ran}");
    let checked = notes(&server, &project, |items| !items.is_empty()).await;
    assert_eq!(checked[0]["path"], "notes.txt");
    assert_eq!(checked[0]["message"], "checked for request");

    // Disabled: its collection is cleared.
    server.post("/plugins/notes/disable", json!({})).await;
    notes(&server, &project, <[Value]>::is_empty).await;
}
