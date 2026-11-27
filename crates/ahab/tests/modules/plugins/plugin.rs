//! `/api/v1/plugins`: installing, enabling, placing and removing plugins.

use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::support::TestApp;

/// The echo test plugin, built by `make plugin-fixtures`.
fn echo() -> PathBuf {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins/echo");
    assert!(
        folder.join("dist/daemon.js").is_file(),
        "the echo plugin is not built: run `make plugin-fixtures`"
    );
    folder
}

/// A copy of echo's installed files in a new folder, to change.
fn echo_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let from = echo();
    std::fs::copy(
        from.join("ahab-plugin.json"),
        dir.path().join("ahab-plugin.json"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("dist")).unwrap();
    for file in ["web.js", "daemon.js", "package.json", "settings.json"] {
        std::fs::copy(
            from.join("dist").join(file),
            dir.path().join("dist").join(file),
        )
        .unwrap();
    }
    dir
}

fn manifest(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("ahab-plugin.json")).unwrap()).unwrap()
}

fn write_manifest(dir: &Path, value: &Value) {
    std::fs::write(dir.join("ahab-plugin.json"), value.to_string()).unwrap();
}

async fn install(app: &TestApp, location: &Path) -> crate::support::TestResponse {
    app.post(
        "/api/v1/plugins",
        json!({ "source": "folder", "location": location }),
    )
    .await
}

#[tokio::test]
async fn a_plugin_is_installed_enabled_disabled_and_removed() {
    let app = TestApp::start().await;
    let installed = install(&app, &echo()).await;
    assert_eq!(installed.status, StatusCode::CREATED, "{}", installed.body);
    let plugin = &installed.body["plugin"];
    assert_eq!(plugin["id"], "echo");
    assert_eq!(plugin["enabled"], false);
    assert_eq!(plugin["entries"], json!(["web", "daemon"]));
    assert_eq!(
        plugin["hooks"],
        json!(["terminals.create", "notifications.send"])
    );
    assert_eq!(installed.body["warnings"], json!([]));

    let list = app.get("/api/v1/plugins").await;
    assert_eq!(list.body["items"][0]["id"], "echo");

    // Its web entry is served from the installed copy.
    let web = app.get("/api/v1/plugins/echo/files/dist/web.js").await;
    assert_eq!(web.status, StatusCode::OK);
    assert!(
        web.headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/javascript")
    );
    assert!(web.text.contains("/plugin-runtime/ahab-plugin-web.js"));
    let outside = app
        .get("/api/v1/plugins/echo/files/src/web/index.tsx")
        .await;
    assert_eq!(
        outside.status,
        StatusCode::NOT_FOUND,
        "sources are not installed"
    );

    let enabled = app.post("/api/v1/plugins/echo/enable", json!({})).await;
    assert_eq!(enabled.body["enabled"], true);
    let refused = app.delete("/api/v1/plugins/echo").await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert_eq!(refused.body["code"], "plugin_enabled");

    let disabled = app.post("/api/v1/plugins/echo/disable", json!({})).await;
    assert_eq!(disabled.body["enabled"], false);
    let removed = app.delete("/api/v1/plugins/echo").await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    let gone = app.get("/api/v1/plugins/echo").await;
    assert_eq!(gone.body["code"], "plugin_not_found");
}

#[tokio::test]
async fn a_plugin_that_breaks_a_rule_is_refused() {
    let app = TestApp::start().await;
    let empty = tempfile::tempdir().unwrap();
    let none = install(&app, empty.path()).await;
    assert_eq!(none.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        none.body.to_string().contains("ahab-plugin.json"),
        "{}",
        none.body
    );

    let copy = echo_copy();
    let mut bad = manifest(copy.path());
    bad["id"] = json!("Echo!");
    bad["hooks"] = json!(["files.write"]);
    write_manifest(copy.path(), &bad);
    let refused = install(&app, copy.path()).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    let fields: Vec<&str> = refused.body["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|error| error["field"].as_str())
        .collect();
    assert_eq!(fields, ["id", "hooks"], "{}", refused.body);

    let reserved = echo_copy();
    let mut named = manifest(reserved.path());
    named["id"] = json!("terminals");
    write_manifest(reserved.path(), &named);
    let refused = install(&app, reserved.path()).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(refused.body["errors"][0]["field"], "id", "{}", refused.body);

    let unbuilt = echo_copy();
    std::fs::remove_file(unbuilt.path().join("dist/daemon.js")).unwrap();
    let refused = install(&app, unbuilt.path()).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        refused.body.to_string().contains("not built"),
        "{}",
        refused.body
    );
}

#[tokio::test]
async fn installing_twice_is_refused_and_updating_takes_the_new_build() {
    let app = TestApp::start().await;
    let copy = echo_copy();
    let first = install(&app, copy.path()).await;
    let digest = first.body["plugin"]["digest"].clone();
    let again = install(&app, copy.path()).await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.body["code"], "plugin_exists");

    app.post("/api/v1/plugins/echo/enable", json!({})).await;
    std::fs::write(
        copy.path().join("dist/web.js"),
        "export default () => {};\n",
    )
    .unwrap();
    let updated = app.post("/api/v1/plugins/echo/update", json!({})).await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.body);
    assert_ne!(updated.body["plugin"]["digest"], digest);
    assert_eq!(updated.body["plugin"]["enabled"], true, "kept enabled");
    let web = app.get("/api/v1/plugins/echo/files/dist/web.js").await;
    assert_eq!(web.text, "export default () => {};\n");
}

#[tokio::test]
async fn a_plugin_is_installed_from_git() {
    let app = TestApp::start().await;
    let copy = echo_copy();
    let git = |args: &[&str]| {
        let done = std::process::Command::new("git")
            .args(args)
            .current_dir(copy.path())
            .output()
            .unwrap();
        assert!(
            done.status.success(),
            "{}",
            String::from_utf8_lossy(&done.stderr)
        );
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "-A"]);
    git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@example.com",
        "commit",
        "-q",
        "-m",
        "echo",
    ]);
    let url = format!("file://{}", copy.path().display());
    let installed = app
        .post(
            "/api/v1/plugins",
            json!({ "source": "git", "location": url }),
        )
        .await;
    assert_eq!(installed.status, StatusCode::CREATED, "{}", installed.body);
    assert_eq!(installed.body["plugin"]["source"], "git");
    assert_eq!(installed.body["plugin"]["location"], url);
}

#[tokio::test]
async fn a_chosen_plugin_runs_on_the_machines_picked() {
    let app = TestApp::start().await;
    let copy = echo_copy();
    let mut chosen = manifest(copy.path());
    chosen["machines"] = json!("chosen");
    write_manifest(copy.path(), &chosen);
    install(&app, copy.path()).await;
    let machine = app
        .state
        .machines
        .ensure_local(
            ahab::modules::machines::MachineKey::from_secret([3; 32]).public_key(),
            "atlas",
        )
        .await
        .unwrap();

    let enabled = app
        .post(
            "/api/v1/plugins/echo/enable",
            json!({ "machines": [machine.id] }),
        )
        .await;
    assert_eq!(enabled.body["placement"], "chosen");
    assert_eq!(enabled.body["machines"], json!([machine.id]));
    let unknown = app
        .patch(
            "/api/v1/plugins/echo",
            json!({ "machines": [uuid::Uuid::now_v7()] }),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);

    // A plugin for every machine cannot be placed.
    let all = echo_copy();
    let mut other = manifest(all.path());
    other["id"] = json!("echo-all");
    write_manifest(all.path(), &other);
    install(&app, all.path()).await;
    let refused = app
        .patch(
            "/api/v1/plugins/echo-all",
            json!({ "machines": [machine.id] }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_plugin_runs_only_with_the_plugins_it_depends_on() {
    let app = TestApp::start().await;
    let echo = echo_copy();
    install(&app, echo.path()).await;
    let user = echo_copy();
    let mut needs = manifest(user.path());
    needs["id"] = json!("echo-user");
    needs["dependsOn"] = json!({ "echo": ">=0.1.0 <0.2.0", "other": ">=1.0.0" });
    write_manifest(user.path(), &needs);
    assert_eq!(install(&app, user.path()).await.status, StatusCode::CREATED);

    let refused = app
        .post("/api/v1/plugins/echo-user/enable", json!({}))
        .await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert_eq!(refused.body["code"], "plugin_dependency_missing");
    let detail = refused.body["detail"].as_str().unwrap();
    assert!(detail.contains("other is not installed"), "{detail}");
    assert!(detail.contains("echo is not enabled"), "{detail}");

    // In range and enabled: it may run.
    needs["dependsOn"] = json!({ "echo": ">=0.1.0 <0.2.0" });
    write_manifest(user.path(), &needs);
    app.post("/api/v1/plugins/echo-user/update", json!({}))
        .await;
    app.post("/api/v1/plugins/echo/enable", json!({})).await;
    let enabled = app
        .post("/api/v1/plugins/echo-user/enable", json!({}))
        .await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.body);

    // What it depends on stays on while it is on.
    let kept = app.post("/api/v1/plugins/echo/disable", json!({})).await;
    assert_eq!(kept.status, StatusCode::CONFLICT);
    assert_eq!(kept.body["code"], "plugin_required");
    app.post("/api/v1/plugins/echo-user/disable", json!({}))
        .await;
    let disabled = app.post("/api/v1/plugins/echo/disable", json!({})).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.body);

    // A version out of range.
    needs["dependsOn"] = json!({ "echo": ">=2.0.0" });
    write_manifest(user.path(), &needs);
    app.post("/api/v1/plugins/echo-user/update", json!({}))
        .await;
    app.post("/api/v1/plugins/echo/enable", json!({})).await;
    let refused = app
        .post("/api/v1/plugins/echo-user/enable", json!({}))
        .await;
    assert!(
        refused.body["detail"]
            .as_str()
            .unwrap()
            .contains("echo 0.1.0 is installed, and it needs >=2.0.0"),
        "{}",
        refused.body
    );
}

#[tokio::test]
async fn settings_that_do_not_fit_the_schema_are_refused() {
    let app = TestApp::start().await;
    let echo = echo_copy();
    install(&app, echo.path()).await;
    let refused = app
        .put(
            "/api/v1/plugins/echo/settings?scope=server",
            json!({ "values": { "greeting": 5, "colour": "red" } }),
        )
        .await;
    assert_eq!(
        refused.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        refused.body
    );
    let mut fields: Vec<&str> = refused.body["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|error| error["field"].as_str())
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["values.colour", "values.greeting"]);

    let saved = app
        .put(
            "/api/v1/plugins/echo/settings?scope=server",
            json!({ "values": { "greeting": "hi", "loud": null } }),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.body);
}

/// A plugin that only contributes a language: its manifest, a
/// grammar and a query, no entries.
fn language_plugin() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("grammars")).unwrap();
    std::fs::create_dir_all(dir.path().join("queries")).unwrap();
    std::fs::write(dir.path().join("grammars/zig.wasm"), b"\0asm\x01\0\0\0").unwrap();
    std::fs::write(
        dir.path().join("queries/highlights.scm"),
        "(comment) @comment\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("notes.txt"), "not installed").unwrap();
    write_manifest(
        dir.path(),
        &json!({
            "id": "zig", "name": "Zig", "version": "0.1.0", "ahab": ">=0.1.1 <0.2.0",
            "contributes": { "languages": [{
                "id": "zig", "name": "Zig",
                "files": { "extensions": ["zig"] },
                "grammar": { "wasm": "grammars/zig.wasm" },
                "queries": { "highlights": "queries/highlights.scm" }
            }] }
        }),
    );
    dir
}

#[tokio::test]
async fn a_plugin_contributes_a_language_with_its_grammar_and_queries() {
    let app = TestApp::start().await;
    let dir = language_plugin();
    let installed = install(&app, dir.path()).await;
    assert_eq!(installed.status, StatusCode::CREATED, "{}", installed.body);
    let plugin = &installed.body["plugin"];
    assert_eq!(plugin["entries"], json!([]));
    assert_eq!(
        plugin["manifest"]["contributes"]["languages"][0]["files"]["extensions"],
        json!(["zig"])
    );

    let grammar = app.get("/api/v1/plugins/zig/files/grammars/zig.wasm").await;
    assert_eq!(grammar.status, StatusCode::OK);
    assert_eq!(grammar.headers["content-type"], "application/wasm");
    let query = app
        .get("/api/v1/plugins/zig/files/queries/highlights.scm")
        .await;
    assert_eq!(query.text, "(comment) @comment\n");
    let other = app.get("/api/v1/plugins/zig/files/notes.txt").await;
    assert_eq!(
        other.status,
        StatusCode::NOT_FOUND,
        "only the files it names"
    );

    // Enabling needs no machine: there is nothing to run.
    let enabled = app.post("/api/v1/plugins/zig/enable", json!({})).await;
    assert_eq!(enabled.body["enabled"], true, "{}", enabled.body);
}

#[tokio::test]
async fn a_language_whose_files_are_missing_is_refused() {
    let app = TestApp::start().await;
    let dir = language_plugin();
    std::fs::remove_file(dir.path().join("queries/highlights.scm")).unwrap();
    let refused = install(&app, dir.path()).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    let text = refused.body.to_string();
    assert!(text.contains("contributes.languages"), "{text}");
    assert!(text.contains("queries/highlights.scm"), "{text}");
}
