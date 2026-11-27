//! Many repositories in a project: checks within the
//! repositories asked for, waiting their turn on the machine, reading
//! Checkstyle reports; active and pinned repositories; servers that start
//! only in pinned ones.

use std::path::Path;
use std::process::Command;

use reqwest::StatusCode;
use serde_json::{Value, json};

use super::{Window, fake_lsp, local_daemon, project};
use crate::support::TestServer;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A Rust-looking repository at `dir`.
fn repository(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
    git(dir, &["init", "-q"]);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "first"]);
}

/// A folder of five repositories (`repos/r1`…`r5`) and a bare repository
/// with two worktrees (`app/main`, `app/fix`): seven checkouts.
fn many() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    for n in 1..=5 {
        repository(&root.join(format!("repos/r{n}")));
    }
    let app = root.join("app");
    std::fs::create_dir_all(&app).unwrap();
    git(
        &app,
        &[
            "clone",
            "-q",
            "--bare",
            &root.join("repos/r1").display().to_string(),
            ".bare",
        ],
    );
    std::fs::write(app.join(".git"), "gitdir: ./.bare\n").unwrap();
    git(&app, &["worktree", "add", "-q", "main", "main"]);
    git(&app, &["worktree", "add", "-q", "-b", "fix", "fix", "main"]);
    (dir, root.display().to_string())
}

/// A check that notes how many run at once, then writes a Checkstyle report
/// and fails, as Gradle does when it finds problems.
fn check(log: &Path) -> Value {
    let script = format!(
        "mkdir -p {log}.d; touch {log}.d/$$; ls {log}.d | wc -l >> {log}; sleep 0.3; rm {log}.d/$$; \
         mkdir -p build/reports/checkstyle; \
         printf '<checkstyle><file name=\"src/main.rs\"><error line=\"1\" column=\"4\" severity=\"warning\" message=\"main is bare\" source=\"x.BareMainCheck\"/></file></checkstyle>' \
           > build/reports/checkstyle/main.xml; exit 1",
        log = log.display()
    );
    json!({
        "id": "fake-check", "name": "Fake check", "kind": "check", "languages": ["rust"],
        "run": {
            "command": ["sh", "-c", script],
            "rootMarkers": ["Cargo.toml"],
            "on": ["request", "agentTurn", "save"],
            "output": { "format": "checkstyle", "reports": ["build/reports/checkstyle/*.xml"] }
        }
    })
}

#[tokio::test]
async fn checks_run_within_the_repositories_asked_two_at_a_time() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let (dir, path) = many();
    let log = dir.path().join("running.log");
    let (status, body) = server
        .put(
            &format!("/machines/{}/tool-settings", machine.id),
            json!({ "records": [check(&log)], "languages": { "rust": { "tools": ["fake-check"] } } }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, id) = project(&server, &path).await;

    // Seven roots, each in its repository or worktree: Check project asks.
    let (status, roots) = server.get(&format!("/projects/{id}/check-roots")).await;
    assert_eq!(status, StatusCode::OK, "{roots}");
    assert_eq!(roots["askAbove"], 5);
    let mut found: Vec<(String, String)> = roots["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["root"].as_str().unwrap().to_owned(),
                r["checkout"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    found.sort();
    assert_eq!(found.len(), 7, "{roots}");
    assert_eq!(found[0], ("app/fix".to_owned(), "app/fix".to_owned()));
    assert_eq!(found[2], ("repos/r1".to_owned(), "repos/r1".to_owned()));

    // Only the repositories chosen.
    let (status, ran) = server
        .post(
            &format!("/projects/{id}/checks"),
            json!({ "checkouts": ["app/main", "repos/r3"] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{ran}");
    let mut roots: Vec<&str> = ran["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["root"].as_str().unwrap())
        .collect();
    roots.sort_unstable();
    assert_eq!(roots, ["app/main", "repos/r3"]);
    assert!(
        ran["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["ok"] == true),
        "{ran}"
    );
    // The report's problem, read with the Checkstyle adapter.
    let (_, problems) = server.get(&format!("/projects/{id}/problems")).await;
    let items = problems["items"].as_array().unwrap();
    let mut paths: Vec<&str> = items.iter().map(|p| p["path"].as_str().unwrap()).collect();
    paths.sort_unstable();
    assert_eq!(paths, ["app/main/src/main.rs", "repos/r3/src/main.rs"]);
    assert_eq!(items[0]["code"], "BareMain");
    assert_eq!(items[0]["severity"], "warning");
    assert_eq!(items[0]["column"], 4);

    // Everywhere: seven checks, never more than two at once.
    std::fs::write(&log, "").unwrap();
    let (_, ran) = server
        .post(&format!("/projects/{id}/checks"), json!({}))
        .await;
    assert_eq!(ran["checks"].as_array().unwrap().len(), 7, "{ran}");
    let counts: Vec<u32> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|line| line.trim().parse().unwrap())
        .collect();
    assert_eq!(counts.len(), 7);
    assert!(counts.iter().all(|n| *n <= 2), "{counts:?}");
    assert!(counts.contains(&2), "two ran at once: {counts:?}");

    // A machine's setting lets one run at a time.
    server
        .put(
            &format!("/machines/{}/tool-settings", machine.id),
            json!({ "records": [check(&log)], "languages": { "rust": { "tools": ["fake-check"] } }, "maxChecks": 1 }),
        )
        .await;
    std::fs::write(&log, "").unwrap();
    server
        .post(
            &format!("/projects/{id}/checks"),
            json!({ "checkouts": ["repos/r1", "repos/r2", "repos/r4"] }),
        )
        .await;
    let counts = std::fs::read_to_string(&log).unwrap();
    assert!(counts.lines().all(|n| n.trim() == "1"), "{counts}");
}

#[tokio::test]
async fn repositories_used_or_pinned_are_active() {
    let server = TestServer::start().await;
    let (_machine, _daemon) = local_daemon(&server, None).await;
    let (_dir, path) = many();
    let (project, id) = project(&server, &path).await;
    let (_, empty) = server
        .get(&format!("/projects/{id}/repository-activity"))
        .await;
    assert_eq!(empty, json!({ "items": [], "activeDays": 14 }));

    // Opening a file in a worktree makes that worktree active.
    let mut window = Window::open(&server, &id).await;
    window
        .send(json!({ "type": "open", "path": "app/fix/src/main.rs", "language": "rust", "version": 1, "text": "fn main() {}\n" }))
        .await;
    let status = window.until(|f| f["type"] == "status").await;
    assert_eq!(status["checkout"], "app/fix", "{status}");
    let mut activity = Value::Null;
    for _ in 0..100 {
        activity = server
            .get(&format!("/projects/{id}/repository-activity"))
            .await
            .1;
        if !activity["items"].as_array().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let used = &activity["items"][0];
    assert_eq!(used["repository"], "app");
    assert_eq!(used["worktree"], "app/fix");
    assert_eq!(used["active"], true);
    assert_eq!(used["pinned"], false);
    assert!(used["lastActiveAt"].is_string());

    // Pinned: active without being used; unpinned: back to the rule.
    let (status, pinned) = server
        .put(
            &format!("/projects/{id}/repository-pins"),
            json!({ "repository": "repos/r2", "pinned": true }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{pinned}");
    let r2 = pinned["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["repository"] == "repos/r2")
        .unwrap();
    assert_eq!(
        (r2["pinned"].clone(), r2["active"].clone()),
        (json!(true), json!(true))
    );
    let (_, unpinned) = server
        .put(
            &format!("/projects/{id}/repository-pins"),
            json!({ "repository": "repos/r2", "pinned": false }),
        )
        .await;
    let r2 = unpinned["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["repository"] == "repos/r2")
        .unwrap();
    assert_eq!(r2["active"], false);
    let (status, _) = server
        .put(
            &format!("/projects/{id}/repository-pins"),
            json!({ "repository": "../elsewhere", "pinned": true }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // How long a repository stays active is the project's.
    let (status, _) = server
        .put(
            &format!("/projects/{project}/tool-settings"),
            json!({ "activeDays": 3 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, activity) = server
        .get(&format!("/projects/{id}/repository-activity"))
        .await;
    assert_eq!(activity["activeDays"], 3);
    let (status, body) = server
        .put(
            &format!("/projects/{project}/tool-settings"),
            json!({ "memoryBudget": 1024 }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a machine's setting: {body}"
    );
}

#[tokio::test]
async fn servers_can_start_only_in_pinned_repositories() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let (dir, path) = many();
    let outside = dir.path().join("lib.rs");
    server
        .put(
            &format!("/machines/{}/tool-settings", machine.id),
            json!({ "records": [fake_lsp(&outside)], "languages": { "rust": { "tools": ["fake-lsp"] } } }),
        )
        .await;
    let (project, id) = project(&server, &path).await;
    server
        .put(
            &format!("/projects/{project}/tool-settings"),
            json!({ "servers": "pinned" }),
        )
        .await;
    let open = json!({ "type": "open", "path": "repos/r1/src/main.rs", "language": "rust", "version": 1, "text": "fn main() {}\n" });

    let mut window = Window::open(&server, &id).await;
    window.send(open.clone()).await;
    let status = window.until(|f| f["type"] == "status").await;
    assert_eq!(status["tools"][0]["state"], "held", "{status}");
    let (_, servers) = server
        .get(&format!("/machines/{}/language-servers", machine.id))
        .await;
    assert_eq!(servers["items"], json!([]), "nothing started");
    assert!(servers["memoryBudget"].as_u64().unwrap() > 0, "{servers}");

    // Pinned (as a whole repository), its files start their server.
    server
        .put(
            &format!("/projects/{id}/repository-pins"),
            json!({ "repository": "repos/r1", "pinned": true }),
        )
        .await;
    let mut window = Window::open(&server, &id).await;
    window.send(open).await;
    let status = window.until(|f| f["type"] == "status").await;
    assert_ne!(status["tools"][0]["state"], "held", "{status}");
    let (_, servers) = server
        .get(&format!("/machines/{}/language-servers", machine.id))
        .await;
    assert_eq!(servers["items"].as_array().unwrap().len(), 1, "{servers}");
    assert!(
        servers["items"][0]["root"]
            .as_str()
            .unwrap()
            .ends_with("repos/r1")
    );

    // Off: none at all.
    server
        .put(
            &format!("/projects/{project}/tool-settings"),
            json!({ "servers": "off" }),
        )
        .await;
    let mut window = Window::open(&server, &id).await;
    window
        .send(json!({ "type": "open", "path": "repos/r2/src/main.rs", "language": "rust", "version": 1, "text": "fn main() {}\n" }))
        .await;
    let status = window.until(|f| f["type"] == "status").await;
    assert_eq!(status["tools"][0]["state"], "held", "{status}");
}
