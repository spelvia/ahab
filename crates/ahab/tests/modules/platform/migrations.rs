//! The schema: migrating a database that has data is a no-op once it is up
//! to date, every migration can run again after stopping half-way, and the
//! foreign keys and JSON formats behave as the stores expect.

use std::path::PathBuf;

use serde_json::json;
use sqlx::Executor;
use time::OffsetDateTime;
use time::macros::datetime;
use uuid::Uuid;

use ahab::modules::machines::machine::model::{Machine, MachineName, PublicKey};
use ahab::modules::machines::machine::store::{DbMachineRepository, MachineRepository};
use ahab::modules::platform::database::store as database;
use ahab::modules::platform::feature::store::{DbFeatureRepository, FeatureRepository};
use ahab::modules::projects::layout::model::{ClientClass, Layout};
use ahab::modules::projects::layout::store::{DbLayoutRepository, LayoutRepository};
use ahab::modules::projects::project::model::{
    Project, ProjectLook, ProjectMode, ProjectName, ProjectState,
};
use ahab::modules::projects::project::store::{DbProjectRepository, ProjectRepository};
use ahab::modules::sessions::chat::model::{Chat, ChatState, ChatStatus, Defaults};
use ahab::modules::sessions::chat::store::{ChatRepository, DbChatRepository};
use ahab::modules::sessions::session::model::{Folder, Session, SessionKind};
use ahab::modules::sessions::session::store::{DbSessionRepository, SessionRepository};
use ahab::modules::sessions::terminal::model::{Terminal, TerminalState};
use ahab::modules::sessions::terminal::store::{DbTerminalRepository, TerminalRepository};
use ahab::shared::database::Db;

use crate::support::TestDatabase;

const AT: OffsetDateTime = datetime!(2026-11-17 09:00:00.123456 UTC);

/// The migration files, oldest first.
fn migrations() -> Vec<(String, String)> {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut files: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read_to_string(&path).unwrap())
        })
        .collect()
}

async fn applied(db: &Db) -> usize {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM _sqlx_migrations")
        .fetch_one(db)
        .await
        .unwrap();
    usize::try_from(sqlx::Row::get::<i64, _>(&row, "n")).unwrap()
}

/// One of each thing the stores keep, written through them.
struct Rows {
    machine: Machine,
    project: Project,
    folder: Folder,
    session: Session,
    chat: Chat,
    terminal_session: Session,
    terminal: Terminal,
    layout: Layout,
    defaults: Defaults,
}

async fn insert_rows(db: &Db) -> Rows {
    let machine = Machine::joined(
        MachineName::parse("box").unwrap(),
        PublicKey::from_bytes(&[7; 32]).unwrap(),
        AT,
    );
    DbMachineRepository::new(db.clone())
        .insert(&machine)
        .await
        .unwrap();
    let project = Project {
        id: Uuid::now_v7(),
        name: ProjectName::parse("ahab").unwrap(),
        description: None,
        state: ProjectState::Active,
        machine_id: machine.id,
        path: "/src/ahab".to_owned(),
        mode: ProjectMode::ReadWrite,
        group: Some("spelvia".to_owned()),
        created_at: AT,
        updated_at: AT,
        look: ProjectLook::default(),
    };
    DbProjectRepository::new(db.clone())
        .insert(&project)
        .await
        .unwrap();

    let sessions = DbSessionRepository::new(db.clone());
    let folder = Folder {
        id: Uuid::now_v7(),
        project_id: project.id,
        name: "Ideas".to_owned(),
        parent_id: None,
        pinned_at: None,
        created_at: AT,
        updated_at: AT,
    };
    sessions.insert_folder(&folder).await.unwrap();
    let new_session = |kind, folder_id| Session {
        id: Uuid::now_v7(),
        project_id: project.id,
        kind,
        title: "First".to_owned(),
        folder_id,
        pinned_at: None,
        archived_at: None,
        created_at: AT,
        updated_at: AT,
    };
    let session = new_session(SessionKind::Chat, Some(folder.id));
    sessions.insert(&session).await.unwrap();
    let terminal_session = new_session(SessionKind::Terminal, None);
    sessions.insert(&terminal_session).await.unwrap();

    let chats = DbChatRepository::new(db.clone());
    let chat = Chat {
        id: Uuid::now_v7(),
        session_id: session.id,
        project_id: project.id,
        machine_id: machine.id,
        agent: "codex".to_owned(),
        native_id: None,
        model: None,
        effort: None,
        mode: None,
        title: "First".to_owned(),
        state: ChatState::Live,
        status: ChatStatus::Starting,
        unrestricted: false,
        current_turn: None,
        open_requests: vec![json!({ "id": "r1" })],
        close_reason: None,
        created_at: AT,
        updated_at: AT,
        closed_at: None,
    };
    chats.insert(&chat).await.unwrap();
    for (seq, kind) in [(1, "turnStart"), (2, "itemEnd")] {
        let event = json!({ "type": kind, "item": { "kind": "edit", "path": "a.rs" } });
        assert!(chats.add_event(chat.id, seq, &event, AT).await.unwrap());
    }
    let defaults = Defaults {
        agent: "codex".to_owned(),
        model: Some("gpt".to_owned()),
        effort: None,
        mode: None,
        updated_at: AT,
    };
    chats.save_defaults(&defaults).await.unwrap();
    // Saved twice: the second save updates the server-wide row.
    chats.save_defaults(&defaults).await.unwrap();

    let terminal = Terminal {
        id: Uuid::now_v7(),
        session_id: terminal_session.id,
        project_id: project.id,
        machine_id: machine.id,
        command: None,
        state: TerminalState::Running,
        exit_code: None,
        cols: 100,
        rows: 30,
        title: None,
        label: None,
        plugin: Some("echo".to_owned()),
        context: Some(json!({ "task": 1 })),
        hooked_by: vec!["echo".to_owned(), "other".to_owned()],
        created_at: AT,
        updated_at: AT,
    };
    DbTerminalRepository::new(db.clone())
        .insert(&terminal)
        .await
        .unwrap();

    let layout = Layout {
        project_id: project.id,
        client_class: ClientClass::Desktop,
        layout: json!({ "windows": [] }),
        previous: None,
        revision: 1,
        updated_at: AT,
    };
    DbLayoutRepository::new(db.clone())
        .save(&layout)
        .await
        .unwrap();

    let features = DbFeatureRepository::new(db.clone());
    features.set("git", false, AT).await.unwrap();
    features.set("git", true, AT).await.unwrap();

    Rows {
        machine,
        project,
        folder,
        session,
        chat,
        terminal_session,
        terminal,
        layout,
        defaults,
    }
}

#[tokio::test]
async fn migrating_again_keeps_everything_as_it_was() {
    let database = TestDatabase::new().await;
    let db = &database.db;
    let rows = insert_rows(db).await;
    assert_eq!(applied(db).await, migrations().len());

    database::migrate(db).await.unwrap();

    assert_eq!(applied(db).await, migrations().len());
    let chats = DbChatRepository::new(db.clone());
    assert_eq!(
        chats.get(rows.chat.id).await.unwrap(),
        Some(rows.chat.clone())
    );
    assert_eq!(chats.events(rows.chat.id, 0, 10).await.unwrap().len(), 2);
    assert_eq!(chats.defaults().await.unwrap(), vec![rows.defaults]);
    assert_eq!(
        DbSessionRepository::new(db.clone())
            .get(rows.session.id)
            .await
            .unwrap(),
        Some(rows.session)
    );
    assert_eq!(
        DbTerminalRepository::new(db.clone())
            .get(rows.terminal.id)
            .await
            .unwrap(),
        Some(rows.terminal)
    );
    assert_eq!(
        DbLayoutRepository::new(db.clone())
            .get(rows.project.id, ClientClass::Desktop)
            .await
            .unwrap(),
        Some(rows.layout)
    );
    assert_eq!(
        DbFeatureRepository::new(db.clone()).all().await.unwrap(),
        vec![("git".to_owned(), true)]
    );
}

#[tokio::test]
async fn every_migration_can_run_again() {
    let database = TestDatabase::empty().await;
    for (name, sql) in migrations() {
        for run in 1..=2 {
            database
                .db
                .execute(sqlx::raw_sql(sqlx::AssertSqlSafe(sql.clone())))
                .await
                .unwrap_or_else(|error| panic!("{name}, run {run}: {error}"));
        }
    }
}

#[tokio::test]
async fn foreign_keys_are_enforced() {
    let database = TestDatabase::new().await;
    let rows = insert_rows(&database.db).await;
    let chats = DbChatRepository::new(database.db.clone());
    let orphan = Chat {
        id: Uuid::now_v7(),
        session_id: Uuid::now_v7(),
        ..rows.chat
    };
    assert!(chats.insert(&orphan).await.is_err(), "no such session");
}

#[tokio::test]
async fn a_deleted_folder_leaves_its_sessions_out_of_any() {
    let database = TestDatabase::new().await;
    let rows = insert_rows(&database.db).await;
    let sessions = DbSessionRepository::new(database.db.clone());

    sessions.delete_folder(rows.folder.id).await.unwrap();

    let session = sessions.get(rows.session.id).await.unwrap().unwrap();
    assert_eq!(session.folder_id, None);
}

#[tokio::test]
async fn deleting_a_session_or_project_takes_what_belongs_to_it() {
    let database = TestDatabase::new().await;
    let db = &database.db;
    let rows = insert_rows(db).await;
    let sessions = DbSessionRepository::new(db.clone());
    let chats = DbChatRepository::new(db.clone());
    let terminals = DbTerminalRepository::new(db.clone());

    sessions.delete(rows.terminal_session.id).await.unwrap();
    assert_eq!(terminals.get(rows.terminal.id).await.unwrap(), None);
    assert!(chats.get(rows.chat.id).await.unwrap().is_some());

    DbProjectRepository::new(db.clone())
        .delete(rows.project.id)
        .await
        .unwrap();
    assert_eq!(chats.get(rows.chat.id).await.unwrap(), None);
    assert_eq!(sessions.get(rows.session.id).await.unwrap(), None);
    assert_eq!(chats.last_seq(rows.chat.id).await.unwrap(), 0, "no events");
}

#[tokio::test]
async fn deleting_a_machine_takes_its_projects_and_chats() {
    let database = TestDatabase::new().await;
    let db = &database.db;
    let rows = insert_rows(db).await;

    DbMachineRepository::new(db.clone())
        .delete(rows.machine.id)
        .await
        .unwrap();

    let projects = DbProjectRepository::new(db.clone());
    assert_eq!(projects.get(rows.project.id).await.unwrap(), None);
    let chats = DbChatRepository::new(db.clone());
    assert_eq!(chats.get(rows.chat.id).await.unwrap(), None);
}

/// Marks every row's `column` as written in format 2, as a newer Ahab would.
async fn newer_format(db: &Db, table: &str, column: &str) {
    db.execute(sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "UPDATE {table} SET {column}_format = 2"
    ))))
    .await
    .unwrap();
}

#[tokio::test]
async fn json_in_a_newer_format_is_never_misread() {
    let database = TestDatabase::new().await;
    let db = &database.db;
    let rows = insert_rows(db).await;
    let chats = DbChatRepository::new(db.clone());

    newer_format(db, "chat_events", "data").await;
    let events = chats.events(rows.chat.id, 0, 10).await.unwrap();
    assert_eq!(events.len(), 2, "the chat still lists");
    assert_eq!(events[0].event["type"], "unreadable");
    assert_eq!(events[0].event["format"], 2);
    assert!(
        events[0].event["data"]
            .as_str()
            .unwrap()
            .contains("turnStart")
    );
    assert!(
        chats
            .finished_items(rows.session.id)
            .await
            .unwrap()
            .is_empty()
    );

    newer_format(db, "chats", "open_requests").await;
    let chat = chats.get(rows.chat.id).await.unwrap().unwrap();
    assert!(chat.open_requests.is_empty());

    newer_format(db, "project_layouts", "layout").await;
    let layouts = DbLayoutRepository::new(db.clone());
    assert_eq!(
        layouts
            .get(rows.project.id, ClientClass::Desktop)
            .await
            .unwrap(),
        None,
        "an unreadable layout counts as none"
    );
    let fresh = Layout {
        layout: json!({ "windows": ["new"] }),
        ..rows.layout.clone()
    };
    assert!(layouts.save(&fresh).await.unwrap());
    assert_eq!(
        layouts
            .get(rows.project.id, ClientClass::Desktop)
            .await
            .unwrap(),
        Some(fresh.clone()),
        "the first save replaces it"
    );
    assert!(
        !layouts.save(&fresh).await.unwrap(),
        "a readable layout is never replaced by a first revision"
    );
}
