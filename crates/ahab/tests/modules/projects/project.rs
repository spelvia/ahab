//! Projects through the API, with a real daemon serving a
//! temporary folder.

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use tokio::sync::watch;

use ahab::daemon::{self, DaemonConfig};
use ahab::modules::machines::MachineKey;
use ahab::modules::machines::presence::api::MachineFacts;

use crate::support::TestServer;

/// The server's own machine, with its daemon running and connected.
pub(super) async fn local_daemon(server: &TestServer) -> watch::Sender<bool> {
    let key = MachineKey::from_secret([7; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let (stop, stopped) = watch::channel(false);
    let config = DaemonConfig {
        server: server.url.clone(),
        key,
        facts: MachineFacts {
            os: "linux".into(),
            arch: "x86_64".into(),
            hostname: "atlas".into(),
            version: "0.1.0".into(),
        },
        server_fingerprint: None,
        data_dir: server.daemon_dir(),
        executable: Some(env!("CARGO_BIN_EXE_ahab").into()),
        plugin_node: None,
        tools: ahab::modules::tools::registry::daemon::ToolOptions {
            login_shell: false,
            ..Default::default()
        },
    };
    tokio::spawn(daemon::run(config, stopped));
    server.online(&machine).await;
    stop
}

/// A folder `app` with a small tree, and a secret next to it.
pub(super) fn folder() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(app.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(app.join("README.md"), "# app\n").unwrap();
    std::fs::write(dir.path().join("secret.txt"), "do not read").unwrap();
    let path = std::fs::canonicalize(&app).unwrap().display().to_string();
    (dir, path)
}

#[tokio::test]
async fn projects_are_created_renamed_grouped_archived_and_deleted() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();

    // Named after its folder unless given a name.
    let (status, created) = server
        .post("/projects", json!({ "machine": "atlas", "path": path }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["state"], "active");
    assert_eq!(created["name"], "app");
    assert_eq!(created["group"], Value::Null);

    // Names need not be unique, and neither need folders.
    let (status, twin) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path, "group": "work" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{twin}");
    assert_eq!(twin["group"], "work");
    assert_ne!(twin["id"], created["id"]);
    let (status, ambiguous) = server.get("/projects/app").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(ambiguous["code"], "project_name_ambiguous");
    let twin_id = twin["id"].as_str().unwrap().to_owned();
    server
        .call(Method::DELETE, &format!("/projects/{twin_id}"), None)
        .await;

    let (status, invalid) = server
        .post(
            "/projects",
            json!({ "name": "a/b", "machine": "atlas", "path": path }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(invalid["code"], "validation_failed");

    // By name or by ID.
    let (status, by_name) = server.get("/projects/app").await;
    assert_eq!(status, StatusCode::OK);
    let id = by_name["id"].as_str().unwrap().to_owned();
    assert_eq!(
        server.get(&format!("/projects/{id}")).await.1["name"],
        "app"
    );

    let (_, renamed) = server
        .call(
            Method::PATCH,
            "/projects/app",
            Some(json!({ "name": "app main", "description": "the platform", "group": "spelvia" })),
        )
        .await;
    assert_eq!(renamed["name"], "app main");
    assert_eq!(renamed["description"], "the platform");
    assert_eq!(renamed["group"], "spelvia");
    // A blank group removes it.
    let (_, ungrouped) = server
        .call(
            Method::PATCH,
            &format!("/projects/{id}"),
            Some(json!({ "group": " " })),
        )
        .await;
    assert_eq!(ungrouped["group"], Value::Null);

    // Archived: hidden from the list.
    let (_, archived) = server
        .call(
            Method::PATCH,
            &format!("/projects/{id}"),
            Some(json!({ "state": "archived" })),
        )
        .await;
    assert_eq!(archived["state"], "archived");
    assert_eq!(server.get("/projects").await.1["items"], json!([]));
    assert_eq!(
        server.get("/projects?state=archived").await.1["items"][0]["name"],
        "app main"
    );
    let (status, restored) = server
        .call(
            Method::PATCH,
            &format!("/projects/{id}"),
            Some(json!({ "state": "active" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["state"], "active");

    let (status, _) = server
        .call(Method::DELETE, &format!("/projects/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        server.get(&format!("/projects/{id}")).await.1["code"],
        "project_not_found"
    );
}

#[tokio::test]
async fn projects_serve_their_folder_and_nothing_else() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();

    let (status, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{project}");
    assert_eq!(project["path"], path);
    assert_eq!(project["machineName"], "atlas");
    assert_eq!(project["mode"], "readWrite");
    assert_eq!(project["name"], "app");
    let id = project["id"].as_str().unwrap().to_owned();

    // A folder that does not exist.
    let (status, missing) = server
        .post(
            "/projects",
            json!({ "machine": "atlas", "path": "/definitely/not/here" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "folder_not_found");

    // Listing and reading, through the daemon.
    let mut listing = Value::Null;
    for _ in 0..50 {
        let (status, body) = server.get(&format!("/projects/{id}/files")).await;
        if status == StatusCode::OK {
            listing = body;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let names: Vec<&str> = listing["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["src", "README.md"]);
    let (_, file) = server
        .get(&format!("/projects/{id}/file?path=src/main.rs"))
        .await;
    assert_eq!(file["content"], "fn main() {}\n");
    assert_eq!(file["binary"], false);

    // Never outside the folder.
    for escape in ["../secret.txt", "src/../../secret.txt", "/etc/passwd"] {
        let (status, refused) = server
            .get(&format!("/projects/{id}/file?path={escape}"))
            .await;
        assert!(
            status == StatusCode::FORBIDDEN || status == StatusCode::UNPROCESSABLE_ENTITY,
            "{escape}: {status} {refused}"
        );
    }

    // Read-only.
    let (_, edited) = server
        .call(
            Method::PATCH,
            &format!("/projects/{id}"),
            Some(json!({ "mode": "readOnly" })),
        )
        .await;
    assert_eq!(edited["mode"], "readOnly");

    // Deleting the project leaves the folder alone.
    let (status, _) = server
        .call(Method::DELETE, &format!("/projects/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(std::path::Path::new(&path).join("README.md").exists());
}

#[tokio::test]
async fn opening_a_folder_finds_or_makes_its_project() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();

    // Without a machine: the server's own.
    let (status, first) = server.post("/projects/open", json!({ "path": path })).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["created"], true);
    assert_eq!(first["project"]["name"], "app");
    assert_eq!(first["project"]["path"], path);

    let (_, second) = server
        .post(
            "/projects/open",
            json!({ "machine": "atlas", "path": path }),
        )
        .await;
    assert_eq!(second["created"], false);
    assert_eq!(second["project"]["id"], first["project"]["id"]);

    // When the folder is two projects, it asks which.
    let (_, other) = server
        .post(
            "/projects",
            json!({ "name": "other", "machine": "atlas", "path": path }),
        )
        .await;
    let (status, ambiguous) = server.post("/projects/open", json!({ "path": path })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(ambiguous["code"], "folder_in_several_projects");
    let (_, chosen) = server
        .post(
            "/projects/open",
            json!({ "path": path, "project": "other" }),
        )
        .await;
    assert_eq!(chosen["project"]["id"], other["id"]);
    assert_eq!(chosen["created"], false);

    // A project of the same name in another folder does not make it
    // ambiguous: only this folder's projects are chosen from.
    let (_elsewhere_dir, elsewhere_path) = folder();
    let (status, elsewhere) = server
        .post(
            "/projects",
            json!({ "name": "other", "machine": "atlas", "path": elsewhere_path }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{elsewhere}");
    let (status, chosen) = server
        .post(
            "/projects/open",
            json!({ "path": path, "project": "other" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{chosen}");
    assert_eq!(chosen["project"]["id"], other["id"]);

    // By ID, or by a name only elsewhere: not this folder's.
    let (status, wrong) = server
        .post(
            "/projects/open",
            json!({ "path": path, "project": elsewhere["id"] }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(wrong["code"], "project_elsewhere");
    server
        .call(
            Method::PATCH,
            &format!("/projects/{}", elsewhere["id"].as_str().unwrap()),
            Some(json!({ "name": "faraway" })),
        )
        .await;
    let (status, wrong) = server
        .post(
            "/projects/open",
            json!({ "path": path, "project": "faraway" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(wrong["code"], "project_elsewhere");

    // Two of this folder's projects with that name: ambiguous among them.
    server
        .post(
            "/projects",
            json!({ "name": "other", "machine": "atlas", "path": path }),
        )
        .await;
    let (status, ambiguous) = server
        .post(
            "/projects/open",
            json!({ "path": path, "project": "other" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(ambiguous["code"], "project_name_ambiguous");
}

#[tokio::test]
async fn the_folder_picker_lists_folders_only() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (dir, _) = folder();
    let parent = std::fs::canonicalize(dir.path())
        .unwrap()
        .display()
        .to_string();

    let (status, home) = server.get("/machines/atlas/fs/home").await;
    assert_eq!(status, StatusCode::OK);
    assert!(home["path"].as_str().unwrap().starts_with('/'));

    let (status, listing) = server
        .get(&format!("/machines/atlas/fs/directories?path={parent}"))
        .await;
    assert_eq!(status, StatusCode::OK, "{listing}");
    let names: Vec<&str> = listing["folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|folder| folder["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["app"]);
}

#[tokio::test]
async fn a_project_keeps_its_windows_per_kind_of_screen() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "ahab", "machine": "atlas", "path": path }),
        )
        .await;
    let id = project["id"].as_str().unwrap().to_owned();
    let desktop = format!("/projects/{id}/layouts/desktop");

    let (status, none) = server.get(&desktop).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(none["code"], "layout_not_found");

    let put = |revision: i64, tiles: u32| {
        server.call(
            Method::PUT,
            &desktop,
            Some(json!({ "layout": { "tiles": tiles }, "revision": revision })),
        )
    };
    let (status, first) = put(0, 1).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["revision"], 1);
    let (status, stale) = put(0, 2).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(stale["code"], "layout_changed");
    let (_, second) = put(1, 3).await;
    assert_eq!(second["revision"], 2);
    assert_eq!(
        server.get(&desktop).await.1["layout"],
        json!({ "tiles": 3 })
    );
    assert_eq!(
        server
            .get(&format!("/projects/{id}/layouts/mobile"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    let (left, right) = tokio::join!(put(2, 4), put(2, 5));
    let (winner, loser) = if left.0 == StatusCode::OK {
        (left, right)
    } else {
        (right, left)
    };
    assert_eq!(winner.0, StatusCode::OK);
    assert_eq!(winner.1["revision"], 3);
    assert_eq!(loser.0, StatusCode::CONFLICT);
    assert_eq!(loser.1["code"], "layout_changed");
    assert_eq!(server.get(&desktop).await.1["layout"], winner.1["layout"]);
}

#[tokio::test]
async fn a_connected_machine_reports_its_agents() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_, machines) = server.get("/machines").await;
    let id = machines["items"][0]["id"].as_str().unwrap().to_owned();

    let (status, agents) = server
        .post(&format!("/machines/{id}/agents/refresh"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{agents}");
    let items = agents["items"].as_array().unwrap();
    let ids: Vec<&str> = items
        .iter()
        .map(|agent| agent["agent"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["claude-code", "codex", "pi"]);
    for agent in items {
        assert!(
            [
                "ready",
                "notSignedIn",
                "unsupportedVersion",
                "notInstalled",
                "error"
            ]
            .contains(&agent["state"].as_str().unwrap())
        );
    }
    // The report is stored.
    assert_eq!(
        server.get(&format!("/machines/{id}/agents")).await.1["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

/// Runs `git` in `dir` for a test's setup.
fn git(dir: &str, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn a_project_is_searched_and_versioned_through_its_machine() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path }),
        )
        .await;
    let id = project["id"].as_str().unwrap().to_owned();

    // Search: contents and the quick open list.
    let (status, found) = server
        .post(
            &format!("/projects/{id}/search"),
            json!({ "query": "MAIN", "wholeWord": true }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["files"][0]["path"], "src/main.rs");
    assert_eq!(found["files"][0]["matches"][0]["line"], 1);
    let (_, exact) = server
        .post(
            &format!("/projects/{id}/search"),
            json!({ "query": "MAIN", "matchCase": true }),
        )
        .await;
    assert_eq!(exact["total"], 0);
    let (status, bad) = server
        .post(
            &format!("/projects/{id}/search"),
            json!({ "query": "(", "regex": true }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    // Streamed: lines of files as found, then how it ended.
    let streamed = server
        .http()
        .post(format!("{}/api/v1/projects/{id}/search/stream", server.url))
        .json(&json!({ "query": "MAIN", "wholeWord": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(streamed.status(), StatusCode::OK);
    let text = streamed.text().await.unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines[0]["type"], "files", "{text}");
    assert_eq!(lines[0]["files"][0]["path"], "src/main.rs");
    assert_eq!(lines.last().unwrap()["type"], "done");
    assert_eq!(lines.last().unwrap()["total"], found["total"]);
    let (status, bad) = server
        .post(
            &format!("/projects/{id}/search/stream"),
            json!({ "query": "(", "regex": true }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a bad pattern fails before streaming: {bad}"
    );
    let (_, index) = server.get(&format!("/projects/{id}/files/index")).await;
    assert_eq!(index["paths"], json!(["README.md", "src/main.rs"]));

    // Git: not a repository, then one.
    let git_path = |rest: &str| format!("/projects/{id}/git/{rest}");
    let (_, none) = server.get(&git_path("status")).await;
    assert_eq!(none["repository"], false);
    // A repository inside the folder is found and used by its path.
    let inner = format!("{path}/lib");
    std::fs::create_dir_all(&inner).unwrap();
    git(&inner, &["init", "-q", "-b", "trunk"]);
    let (_, found) = server.get(&git_path("repositories")).await;
    assert_eq!(found["inside"], false);
    assert_eq!(found["repositories"][0]["path"], "lib");
    let (_, nested) = server.get(&git_path("status?repository=lib")).await;
    assert_eq!(nested["repository"], true);
    assert_eq!(nested["branch"], "trunk");
    std::fs::remove_dir_all(&inner).unwrap();
    let (status, _) = server.post(&git_path("init"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    git(&path, &["config", "user.name", "Ahab Test"]);
    git(&path, &["config", "user.email", "test@example.com"]);
    git(&path, &["config", "commit.gpgsign", "false"]);
    let (_, fresh) = server.get(&git_path("status")).await;
    assert_eq!(fresh["repository"], true);
    assert_eq!(fresh["untracked"].as_array().unwrap().len(), 2);

    let (status, _) = server
        .post(&git_path("stage"), json!({ "all": true }))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = server
        .post(&git_path("commit"), json!({ "message": "First commit" }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = server
        .post(&git_path("commit"), json!({ "message": "Again" }))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "nothing_to_commit");

    std::fs::write(format!("{path}/src/main.rs"), "fn main() { run(); }\n").unwrap();
    let (_, changed) = server.get(&git_path("status")).await;
    assert_eq!(changed["changes"][0]["path"], "src/main.rs");
    assert_eq!(changed["changes"][0]["added"], 1);
    let (_, diff) = server
        .get(&git_path("diff?path=src/main.rs&group=changes"))
        .await;
    assert_eq!(diff["old"], "fn main() {}\n");
    assert_eq!(diff["new"], "fn main() { run(); }\n");

    let (_, log) = server.get(&git_path("log")).await;
    assert_eq!(log["commits"][0]["subject"], "First commit");
    let hash = log["commits"][0]["hash"].as_str().unwrap();
    let (_, shown) = server.get(&git_path(&format!("commits/{hash}"))).await;
    assert_eq!(shown["files"].as_array().unwrap().len(), 2);

    let (status, _) = server
        .post(&git_path("branches"), json!({ "name": "feature/search" }))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, branches) = server.get(&git_path("branches")).await;
    assert_eq!(branches["current"], "feature/search");
    let (status, refused) = server
        .post(&git_path("discard"), json!({ "paths": ["../secret.txt"] }))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    let (status, _) = server
        .post(&git_path("discard"), json!({ "paths": ["src/main.rs"] }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        std::fs::read_to_string(format!("{path}/src/main.rs")).unwrap(),
        "fn main() {}\n"
    );
}

/// Reads an attachment until its output shows `wanted`.
async fn read_until<S>(socket: &mut S, wanted: &str) -> String
where
    S: futures_util::Stream<
            Item = Result<
                tokio_tungstenite::tungstenite::Message,
                tokio_tungstenite::tungstenite::Error,
            >,
        > + Unpin,
{
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
    let mut seen = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while !seen.contains(wanted) {
        let message = tokio::time::timeout_at(deadline, socket.next())
            .await
            .unwrap_or_else(|_| panic!("no {wanted:?} in {seen:?}"))
            .unwrap()
            .unwrap();
        if let Message::Binary(bytes) = message {
            seen.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    seen
}

#[tokio::test]
async fn a_terminal_runs_in_its_project_shared_by_every_attachment() {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;

    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path }),
        )
        .await;
    let project = project["id"].as_str().unwrap().to_owned();

    let (status, session) = server
        .post(
            &format!("/projects/{project}/terminals"),
            json!({ "command": "printf '\\033]2;copying\\007'; cat", "title": "cat", "cols": 100, "rows": 30 }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    assert_eq!(session["kind"], "terminal");
    assert_eq!(session["title"], "cat");
    assert_eq!(session["terminal"]["state"], "running");
    let terminal = session["terminal"]["id"].as_str().unwrap().to_owned();
    let session_id = session["id"].as_str().unwrap().to_owned();

    // The title its shell (here, printf) sets reaches the server.
    let mut title = Value::Null;
    for _ in 0..100 {
        let (_, found) = server.get(&format!("/terminals/{terminal}")).await;
        title = found["title"].clone();
        if title == "copying" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(title, "copying");

    // Listed with the project's sessions.
    let (_, list) = server.get("/projects/app/sessions").await;
    assert_eq!(list["items"][0]["kind"], "terminal");

    // What one attachment types, it and a later one see.
    let attach = format!(
        "{}/api/v1/terminals/{terminal}/attach",
        server.url.replace("http", "ws")
    );
    let mut first = server.ws(&attach).await.unwrap();
    first
        .send(Message::Binary(b"ahab-terminal\n".to_vec().into()))
        .await
        .unwrap();
    read_until(&mut first, "ahab-terminal").await;
    let mut second = server.ws(&attach).await.unwrap();
    read_until(&mut second, "ahab-terminal").await;

    // Killed, it is reported exited and kept.
    let (status, _) = server
        .post(&format!("/terminals/{terminal}/kill"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let mut state = Value::Null;
    for _ in 0..100 {
        let (_, found) = server.get(&format!("/terminals/{terminal}")).await;
        state = found["state"].clone();
        if state == "exited" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(state, "exited");

    // Archived, then deleted: the terminal goes with its session.
    server
        .post(&format!("/sessions/{session_id}/archive"), json!({}))
        .await;
    let (status, _) = server
        .call(Method::DELETE, &format!("/sessions/{session_id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = server.get(&format!("/terminals/{terminal}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The next server-sent event's data on `response`, within five seconds.
pub(super) async fn next_event(
    response: &mut reqwest::Response,
    buffer: &mut String,
) -> serde_json::Value {
    let read = async {
        loop {
            if let Some(end) = buffer.find("\n\n") {
                let block: String = buffer.drain(..end + 2).collect();
                if let Some(data) = block.lines().find_map(|line| line.strip_prefix("data: ")) {
                    return serde_json::from_str(data).unwrap();
                }
                continue;
            }
            let Some(chunk) = response.chunk().await.unwrap() else {
                panic!("the stream ended");
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
    };
    let Ok(event) = tokio::time::timeout(std::time::Duration::from_secs(5), read).await else {
        panic!("no event within five seconds");
    };
    event
}

#[tokio::test]
async fn a_checkout_is_watched_and_its_changes_told() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();
    git(&path, &["init", "-q", "-b", "main"]);
    std::fs::create_dir_all(format!("{path}/node_modules/pkg")).unwrap();
    std::fs::write(format!("{path}/.gitignore"), "node_modules/\n").unwrap();
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path }),
        )
        .await;
    let id = project["id"].as_str().unwrap().to_owned();

    let mut stream = server
        .http()
        .get(format!("{}/api/v1/projects/{id}/git/watch", server.url))
        .send()
        .await
        .unwrap();
    assert_eq!(stream.status(), StatusCode::OK);
    let mut buffer = String::new();
    assert_eq!(
        next_event(&mut stream, &mut buffer).await["type"],
        "watching"
    );

    // A file in a watched folder, then a new folder's file, then Git's own.
    std::fs::write(format!("{path}/src/main.rs"), "fn main() { todo!() }\n").unwrap();
    assert_eq!(
        next_event(&mut stream, &mut buffer).await["type"],
        "changed"
    );
    std::fs::create_dir_all(format!("{path}/src/new")).unwrap();
    assert_eq!(
        next_event(&mut stream, &mut buffer).await["type"],
        "changed"
    );
    std::fs::write(format!("{path}/src/new/file.rs"), "// new\n").unwrap();
    assert_eq!(
        next_event(&mut stream, &mut buffer).await["type"],
        "changed"
    );
    git(&path, &["add", "-A"]);
    assert_eq!(
        next_event(&mut stream, &mut buffer).await["type"],
        "changed"
    );

    // Not a repository: refused before streaming.
    let other = tempfile::tempdir().unwrap();
    let plain = std::fs::canonicalize(other.path())
        .unwrap()
        .display()
        .to_string();
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "plain", "machine": "atlas", "path": plain }),
        )
        .await;
    let plain_id = project["id"].as_str().unwrap().to_owned();
    let (status, body) = server.get(&format!("/projects/{plain_id}/git/watch")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "not_a_repository");
}

#[tokio::test]
async fn projects_are_ordered_by_hand_and_wear_their_own_mark() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();
    let mut ids = Vec::new();
    for name in ["beta", "alpha", "gamma"] {
        let (_, created) = server
            .post(
                "/projects",
                json!({ "name": name, "machine": "atlas", "path": path }),
            )
            .await;
        ids.push(created["id"].as_str().unwrap().to_owned());
    }
    let names = |list: &Value| -> Vec<String> {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap().to_owned())
            .collect()
    };
    // By name, until ordered by hand; those left out come after, by name.
    assert_eq!(
        names(&server.get("/projects").await.1),
        ["alpha", "beta", "gamma"]
    );
    let (status, _) = server
        .put("/project-order", json!({ "ids": [ids[2], ids[0]] }))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        names(&server.get("/projects").await.1),
        ["gamma", "beta", "alpha"]
    );
    let (status, _) = server
        .put("/project-order", json!({ "ids": [ids[0], ids[0]] }))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // An emoji, then an image (served at the address it gives), then none.
    let (status, project) = server
        .put("/projects/alpha/icon", json!({ "emoji": "🐋" }))
        .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    assert_eq!(project["icon"], json!({ "emoji": "🐋", "image": null }));
    let png = [&b"\x89PNG\r\n\x1a\n"[..], &[0; 8]].concat();
    let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png);
    let (status, project) = server
        .put(
            "/projects/alpha/icon",
            json!({ "image": format!("data:image/png;base64,{data}") }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    let address = project["icon"]["image"].as_str().unwrap().to_owned();
    assert!(project["icon"]["emoji"].is_null());
    let image = server
        .http()
        .get(format!("{}{address}", server.url))
        .send()
        .await
        .unwrap();
    assert_eq!(image.status(), StatusCode::OK);
    assert_eq!(image.headers()["content-type"], "image/png");
    assert_eq!(image.bytes().await.unwrap().as_ref(), png.as_slice());
    let (status, _) = server
        .put(
            "/projects/alpha/icon",
            json!({ "image": "data:image/svg+xml;base64,PHN2Zy8+" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, project) = server.delete("/projects/alpha/icon").await;
    assert_eq!(status, StatusCode::OK);
    assert!(project["icon"].is_null());
}
