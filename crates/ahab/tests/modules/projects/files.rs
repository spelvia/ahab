//! Editing a project's files through the API, with a real daemon:
//! versioned saves and their conflicts, read-only projects,
//! creating, renaming and deleting, and watching open files.

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use ahab::modules::machines::MachineKey;

use super::project::{folder, local_daemon, next_event};
use crate::support::{FakeDaemon, TestServer};

/// A project `app` on the server's own machine, its folder, and the
/// project's ID once its machine serves it.
async fn project(server: &TestServer, path: &str) -> String {
    let (status, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{project}");
    let id = project["id"].as_str().unwrap().to_owned();
    for _ in 0..50 {
        if server.get(&format!("/projects/{id}/files")).await.0 == StatusCode::OK {
            return id;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the project was never served");
}

async fn save(server: &TestServer, id: &str, path: &str, body: Value) -> (StatusCode, Value) {
    server
        .call(
            Method::PUT,
            &format!("/projects/{id}/file?path={path}"),
            Some(body),
        )
        .await
}

#[tokio::test]
async fn a_save_goes_only_over_the_version_it_started_from() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();
    let id = project(&server, &path).await;

    let (_, file) = server
        .get(&format!("/projects/{id}/file?path=src/main.rs"))
        .await;
    let start = file["version"].as_str().unwrap().to_owned();
    assert!(!file["truncated"].as_bool().unwrap());

    let (status, saved) = save(
        &server,
        &id,
        "src/main.rs",
        json!({ "content": "fn main() { run(); }\n", "expectedVersion": start }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let mine = saved["version"].as_str().unwrap().to_owned();
    assert_ne!(mine, start);
    assert_eq!(
        std::fs::read_to_string(format!("{path}/src/main.rs")).unwrap(),
        "fn main() { run(); }\n"
    );
    let (_, again) = server
        .get(&format!("/projects/{id}/file?path=src/main.rs"))
        .await;
    assert_eq!(again["version"], mine.as_str());

    // Another client saves from the version it started from: refused.
    let (status, refused) = save(
        &server,
        &id,
        "src/main.rs",
        json!({ "content": "theirs", "expectedVersion": start }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refused["code"], "file_changed");

    // An agent changes the file on disk: the next save is refused too.
    std::fs::write(format!("{path}/src/main.rs"), "fn main() { agent(); }\n").unwrap();
    let (status, refused) = save(
        &server,
        &id,
        "src/main.rs",
        json!({ "content": "mine again", "expectedVersion": mine }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refused["code"], "file_changed");
    assert_eq!(
        std::fs::read_to_string(format!("{path}/src/main.rs")).unwrap(),
        "fn main() { agent(); }\n"
    );

    // Save anyway.
    let (status, _) = save(&server, &id, "src/main.rs", json!({ "content": "mine" })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        std::fs::read_to_string(format!("{path}/src/main.rs")).unwrap(),
        "mine"
    );
}

#[tokio::test]
async fn files_are_created_renamed_and_deleted_except_in_read_only_projects() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();
    let id = project(&server, &path).await;

    let (status, made) = server
        .post(
            &format!("/projects/{id}/files"),
            json!({ "path": "docs/intro.md", "kind": "file", "content": "# Intro\n" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["path"], "docs/intro.md");
    assert_eq!(made["kind"], "file");
    let (status, exists) = server
        .post(
            &format!("/projects/{id}/files"),
            json!({ "path": "docs/intro.md", "kind": "file" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(exists["code"], "already_exists");
    let (status, _) = server
        .post(
            &format!("/projects/{id}/files"),
            json!({ "path": "assets", "kind": "folder" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(std::path::Path::new(&path).join("assets").is_dir());

    let (status, moved) = server
        .post(
            &format!("/projects/{id}/files/rename"),
            json!({ "from": "docs/intro.md", "to": "docs/start.md" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["path"], "docs/start.md");
    let (status, taken) = server
        .post(
            &format!("/projects/{id}/files/rename"),
            json!({ "from": "docs/start.md", "to": "README.md" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(taken["code"], "already_exists");

    let (status, _) = server
        .call(
            Method::DELETE,
            &format!("/projects/{id}/file?path=docs/start.md&expectedVersion=stale"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = server
        .call(
            Method::DELETE,
            &format!("/projects/{id}/file?path=docs"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!std::path::Path::new(&path).join("docs").exists());
    let (status, gone) = server
        .get(&format!("/projects/{id}/file?path=docs/start.md"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{gone}");

    // Never outside the folder.
    let (status, _) = server
        .post(
            &format!("/projects/{id}/files"),
            json!({ "path": "../planted.txt", "kind": "file" }),
        )
        .await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::UNPROCESSABLE_ENTITY,
        "{status}"
    );

    // A read-only project refuses every change, and changes nothing.
    let (_, edited) = server
        .call(
            Method::PATCH,
            &format!("/projects/{id}"),
            Some(json!({ "mode": "readOnly" })),
        )
        .await;
    assert_eq!(edited["mode"], "readOnly");
    let answers = [
        save(&server, &id, "README.md", json!({ "content": "x" })).await,
        server
            .post(
                &format!("/projects/{id}/files"),
                json!({ "path": "new.txt", "kind": "file" }),
            )
            .await,
        server
            .post(
                &format!("/projects/{id}/files/rename"),
                json!({ "from": "README.md", "to": "R.md" }),
            )
            .await,
        server
            .call(
                Method::DELETE,
                &format!("/projects/{id}/file?path=README.md"),
                None,
            )
            .await,
    ];
    for (status, body) in answers {
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(body["code"], "read_only");
    }
    assert_eq!(
        std::fs::read_to_string(format!("{path}/README.md")).unwrap(),
        "# app\n"
    );
    // Reading still works.
    let (status, _) = server
        .get(&format!("/projects/{id}/file?path=README.md"))
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn open_files_are_watched_and_their_changes_told() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();
    let id = project(&server, &path).await;
    let (_, file) = server
        .get(&format!("/projects/{id}/file?path=src/main.rs"))
        .await;

    let mut stream = server
        .http()
        .get(format!(
            "{}/api/v1/projects/{id}/files/watch?path=src/main.rs&path=README.md",
            server.url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(stream.status(), StatusCode::OK);
    let mut buffer = String::new();
    let watching = next_event(&mut stream, &mut buffer).await;
    assert_eq!(watching["type"], "watching");
    assert_eq!(watching["versions"]["src/main.rs"], file["version"]);

    // A change on disk, as an agent makes it.
    std::fs::write(format!("{path}/src/main.rs"), "fn main() { todo!() }\n").unwrap();
    let changed = next_event(&mut stream, &mut buffer).await;
    assert_eq!(changed["type"], "changed");
    assert_eq!(changed["path"], "src/main.rs");
    let (_, now) = server
        .get(&format!("/projects/{id}/file?path=src/main.rs"))
        .await;
    assert_eq!(changed["version"], now["version"]);

    // A save through Ahab is told too: the window that saved knows its
    // version and ignores it.
    let (_, saved) = save(&server, &id, "README.md", json!({ "content": "# new\n" })).await;
    let changed = next_event(&mut stream, &mut buffer).await;
    assert_eq!(changed["path"], "README.md");
    assert_eq!(changed["version"], saved["version"]);

    // Deleted: no version.
    std::fs::remove_file(format!("{path}/src/main.rs")).unwrap();
    let gone = next_event(&mut stream, &mut buffer).await;
    assert_eq!(gone["path"], "src/main.rs");
    assert_eq!(gone["version"], Value::Null);
}

#[tokio::test]
async fn files_and_folders_move_or_copy_by_dragging() {
    let server = TestServer::start().await;
    let _daemon = local_daemon(&server).await;
    let (_dir, path) = folder();
    let root = std::path::Path::new(&path);
    let id = project(&server, &path).await;
    let move_to = |body: Value| {
        let server = &server;
        let url = format!("/projects/{id}/files/move");
        async move { server.post(&url, body).await }
    };
    std::fs::create_dir_all(root.join("lib/nested")).unwrap();
    std::fs::write(root.join("lib/nested/a.txt"), "a").unwrap();
    std::fs::write(root.join("notes.md"), "mine").unwrap();
    std::fs::write(root.join("lib/notes.md"), "theirs").unwrap();

    // Into a folder, its missing folders made.
    let (status, moved) = move_to(json!({ "from": "lib/nested", "to": "pkg/deep/nested" })).await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["path"], "pkg/deep/nested");
    assert_eq!(
        std::fs::read_to_string(root.join("pkg/deep/nested/a.txt")).unwrap(),
        "a"
    );
    assert!(!root.join("lib/nested").exists());

    // Never into itself, nor onto itself.
    let (status, body) = move_to(json!({ "from": "pkg", "to": "pkg/deep/pkg" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, _) = move_to(json!({ "from": "pkg", "to": "pkg" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // A name already there: refused, unless replacing.
    let (status, taken) = move_to(json!({ "from": "notes.md", "to": "lib/notes.md" })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(taken["code"], "already_exists");
    let (status, _) =
        move_to(json!({ "from": "notes.md", "to": "lib/notes.md", "replace": true })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        std::fs::read_to_string(root.join("lib/notes.md")).unwrap(),
        "mine"
    );
    assert!(!root.join("notes.md").exists());
    // Never over a folder that holds it.
    std::fs::create_dir_all(root.join("x/x")).unwrap();
    std::fs::write(root.join("x/x/keep.txt"), "kept").unwrap();
    let (status, _) = move_to(json!({ "from": "x/x", "to": "x", "replace": true })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(root.join("x/x/keep.txt").exists());

    // A copy leaves the original, folders with everything in them.
    let (status, copied) = move_to(json!({ "from": "pkg", "to": "lib/pkg", "copy": true })).await;
    assert_eq!(status, StatusCode::OK, "{copied}");
    assert_eq!(
        std::fs::read_to_string(root.join("lib/pkg/deep/nested/a.txt")).unwrap(),
        "a"
    );
    assert!(root.join("pkg/deep/nested/a.txt").exists());
    let (status, _) = move_to(json!({ "from": "pkg", "to": "lib/pkg", "copy": true })).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Never outside the folder.
    let (status, _) = move_to(json!({ "from": "README.md", "to": "../R.md" })).await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::UNPROCESSABLE_ENTITY,
        "{status}"
    );

    // Read-only: refused, nothing changed.
    server
        .call(
            Method::PATCH,
            &format!("/projects/{id}"),
            Some(json!({ "mode": "readOnly" })),
        )
        .await;
    let (status, body) = move_to(json!({ "from": "README.md", "to": "lib/README.md" })).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "read_only");
    assert!(root.join("README.md").exists());
}

#[tokio::test]
async fn an_older_machine_is_never_asked_to_write() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([11; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    // A daemon from before capabilities: protocol 1's methods only.
    let daemon = FakeDaemon::connect(&server, &key).await;
    server.online(&machine).await;
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": "/work/app" }),
        )
        .await;
    let id = project["id"].as_str().unwrap().to_owned();

    let (status, body) = save(&server, &id, "a.txt", json!({ "content": "x" })).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "machine_outdated");
    let (status, body) = server
        .get(&format!("/projects/{id}/files/watch?path=a.txt"))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "machine_outdated");
    let (status, body) = server
        .post(
            &format!("/projects/{id}/files/move"),
            json!({ "from": "a.txt", "to": "b/a.txt", "copy": true }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "machine_outdated");
    assert!(daemon.asked_for("fs.write").is_empty());
    assert!(daemon.asked_for("fs.watch").is_empty());
    assert!(daemon.asked_for("fs.move").is_empty());
}
