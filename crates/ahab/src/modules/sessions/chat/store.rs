//! Storing sessions and their events. The service sees only
//! [`ChatRepository`].

use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::any::AnyRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::sessions::chat::model::{
    Chat, ChatState, ChatStatus, Defaults, MessageHit, MessageRole, Submission, SubmissionState,
    message_of,
};
use crate::shared::database::{Db, RowExt, assembled, changed_one, json_text};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

/// The format of the events this Ahab writes (`chat_events.data_format`).
pub const EVENT_FORMAT: i32 = 1;
/// The format of a chat's open requests (`chats.open_requests_format`).
pub const OPEN_REQUESTS_FORMAT: i32 = 1;
/// The format of the models kept (`agent_models.models_format`).
pub const MODELS_FORMAT: i32 = 1;
const SUBMISSION_ATTACHMENTS_FORMAT: i32 = 1;

/// One stored event of a session.
///
/// An event a newer Ahab wrote in a format this one does not know is kept
/// as it is, marked unreadable: `{"type": "unreadable", "format": n,
/// "data": "<the stored JSON>"}`. Clients skip event types they do not know,
/// so the rest of the chat still shows.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    pub seq: i64,
    pub event: Value,
    pub created_at: OffsetDateTime,
}

#[async_trait]
pub trait ChatRepository: Send + Sync {
    async fn get(&self, id: Uuid) -> Result<Option<Chat>, RepoError>;
    async fn submission(&self, chat: Uuid, id: Uuid) -> Result<Option<Submission>, RepoError>;
    async fn add_submission(&self, chat: Uuid, message: &Submission) -> Result<bool, RepoError>;
    async fn queued(&self, chat: Uuid) -> Result<Vec<Submission>, RepoError>;
    async fn promote_submission(&self, chat: Uuid, id: Uuid) -> Result<bool, RepoError>;
    async fn claim_submission(
        &self,
        chat: Uuid,
        id: Uuid,
        after: Option<i64>,
    ) -> Result<bool, RepoError>;
    async fn update_submission(
        &self,
        chat: Uuid,
        id: Uuid,
        before: SubmissionState,
        after: SubmissionState,
    ) -> Result<bool, RepoError>;
    async fn forget_submission(&self, chat: Uuid, id: Uuid) -> Result<(), RepoError>;
    /// Puts the waiting messages in `order` (the ones it names; an
    /// interrupting message stays first).
    async fn reorder_queue(&self, chat: Uuid, order: &[Uuid]) -> Result<(), RepoError>;
    /// Changes a message still waiting: whether it was still there to change.
    async fn edit_queued(
        &self,
        chat: Uuid,
        id: Uuid,
        shown: &str,
        prompt: &str,
    ) -> Result<bool, RepoError>;
    async fn withdraw_queue(&self, chat: Uuid) -> Result<bool, RepoError>;
    async fn recover_submissions(&self) -> Result<(), RepoError>;
    /// A project's sessions, most recently changed first.
    async fn list_for_project(&self, project: Uuid) -> Result<Vec<Chat>, RepoError>;
    /// A session's chats, most recently changed first.
    async fn list_for_session(&self, session: Uuid) -> Result<Vec<Chat>, RepoError>;
    /// The live sessions on a machine.
    async fn live_on_machine(&self, machine: Uuid) -> Result<Vec<Chat>, RepoError>;
    /// Every session with an open request, for the "needs you" counts.
    async fn waiting(&self) -> Result<Vec<Chat>, RepoError>;
    /// How many live chats are starting or running a turn.
    async fn working(&self) -> Result<u64, RepoError>;
    async fn save_fork(
        &self,
        source: &Chat,
        native: &str,
        seq: i64,
        new_session: bool,
        now: OffsetDateTime,
    ) -> Result<Uuid, RepoError>;
    async fn insert(&self, session: &Chat) -> Result<(), RepoError>;
    async fn update(&self, session: &Chat) -> Result<(), RepoError>;
    /// Notes that someone named the chat, so an agent's title leaves it.
    async fn mark_named(&self, id: Uuid) -> Result<(), RepoError>;
    /// The person's messages in a chat, oldest first: where each is in
    /// its history and its text.
    async fn user_messages(&self, chat: Uuid) -> Result<Vec<(i64, String)>, RepoError>;
    async fn is_named(&self, id: Uuid) -> Result<bool, RepoError>;
    /// Stores only what an agent's event changes (`Chat::apply`): its state,
    /// status, turn and requests. Events arrive while other steps write the
    /// chat, so a whole-row write from an older copy would undo theirs (it
    /// lost chats' `native_id`, so they could not be continued).
    async fn update_activity(&self, chat: &Chat) -> Result<(), RepoError>;
    /// Stores what the daemon started: the agent's session, model, effort and
    /// mode.
    /// A starting chat is then working; any other status is left as events
    /// set it.
    async fn record_started(&self, chat: &Chat) -> Result<(), RepoError>;
    /// Every agent's server-wide defaults.
    async fn defaults(&self) -> Result<Vec<Defaults>, RepoError>;
    async fn save_defaults(&self, defaults: &Defaults) -> Result<(), RepoError>;
    /// The number of the last kept event; 0 for none.
    async fn last_seq(&self, chat: Uuid) -> Result<i64, RepoError>;
    /// Deletes a session's chats and their events.
    async fn delete_for_session(&self, session: Uuid) -> Result<(), RepoError>;
    /// Deletes one chat and its events.
    async fn delete(&self, chat: Uuid) -> Result<(), RepoError>;
    /// A chat's fast mode, if one was chosen (else the provider's default).
    async fn fast(&self, chat: Uuid) -> Result<Option<bool>, RepoError>;
    async fn set_fast(&self, chat: Uuid, fast: Option<bool>) -> Result<(), RepoError>;
    /// Files a chat, and its searchable messages, in another session.
    async fn move_to(&self, chat: Uuid, session: Uuid) -> Result<(), RepoError>;
    /// Stores an event once; whether it was new.
    async fn add_event(
        &self,
        session: Uuid,
        seq: i64,
        event: &Value,
        at: OffsetDateTime,
    ) -> Result<bool, RepoError>;
    /// The finished items (`itemEnd` events) of a session's chats, oldest
    /// first: where its agents' edits are recorded. Unreadable ones are
    /// left out.
    async fn finished_items(&self, session: Uuid) -> Result<Vec<Value>, RepoError>;
    /// A chat's kept events of one `kind` (`turnAccounting`), oldest first;
    /// unreadable ones are left out.
    async fn events_of_kind(&self, chat: Uuid, kind: &str) -> Result<Vec<Value>, RepoError>;
    /// A session's events after `after`, oldest first, at most `limit`.
    async fn events(
        &self,
        session: Uuid,
        after: i64,
        limit: u64,
    ) -> Result<Vec<StoredEvent>, RepoError>;

    /// Kept events before a cursor, newest first.
    async fn events_before(
        &self,
        chat: Uuid,
        before: i64,
        limit: u64,
    ) -> Result<Vec<StoredEvent>, RepoError>;
    /// The messages of a project's sessions that hold every one of `terms`
    /// (lowercased), newest first, at most `limit`.
    async fn search_messages(
        &self,
        project: Uuid,
        terms: &[String],
        limit: u64,
    ) -> Result<Vec<MessageHit>, RepoError>;
    /// A project's sessions whose title holds every one of `terms`:
    /// (session, project, title, last changed), newest first.
    async fn search_titles(
        &self,
        project: Uuid,
        terms: &[String],
        limit: u64,
    ) -> Result<Vec<(Uuid, Uuid, String, OffsetDateTime)>, RepoError>;
    /// Takes the messages kept before searching existed into the search,
    /// once: only while none are there yet. How many it took.
    async fn index_messages(&self) -> Result<u64, RepoError>;
}

/// `term` as a `LIKE` pattern that finds it anywhere, `\` escaping.
fn contains_pattern(term: &str) -> String {
    let mut pattern = String::with_capacity(term.len() + 2);
    pattern.push('%');
    for c in term.chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

/// `column LIKE $n ESCAPE '\'` for each term, from `$first` on, joined by AND.
fn all_terms(column: &str, terms: usize, first: usize) -> String {
    (0..terms)
        .map(|at| format!("{column} LIKE ${} ESCAPE '\\'", first + at))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// A chat's columns, in the order [`DbChatRepository::write`] binds them.
const COLUMNS: &str = "id, session_id, project_id, machine_id, agent, native_id, \
    model, effort, mode, title, state, status, unrestricted, current_turn, open_requests, \
    close_reason, created_at, updated_at, closed_at, open_requests_format";

/// The same, as [`to_session`] reads them.
const SELECTED: &str = "id, session_id, project_id, machine_id, agent, native_id, \
    model, effort, mode, title, state, status, CAST(unrestricted AS INTEGER) AS unrestricted, \
    current_turn, open_requests, open_requests_format, close_reason, created_at, updated_at, \
    closed_at";

fn to_submission(row: &AnyRow) -> Result<Submission, RepoError> {
    let state: String = row.value("state")?;
    let state = SubmissionState::parse(&state)
        .ok_or_else(|| RepoError::InvalidData(format!("unknown submission state {state}")))?;
    if row.format("attachments")? != SUBMISSION_ATTACHMENTS_FORMAT {
        return Err(RepoError::InvalidData(
            "unknown submission attachment format".to_owned(),
        ));
    }
    if row.format("command")? != 1 {
        return Err(RepoError::InvalidData("unknown command format".into()));
    }
    Ok(Submission {
        command: row.json("command")?,
        id: row.uuid("id")?,
        fingerprint: row.value("fingerprint")?,
        state,
        queued: row.value::<Option<i64>>("queue_position")?.is_some(),
        interrupt: row.flag("interrupting")?,
        shown: row.value("shown")?,
        prompt: row.value("prompt")?,
        attachments: row.json("attachments")?,
    })
}

pub struct DbChatRepository {
    db: Db,
}

impl DbChatRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Keeps a finished message for searching, with its chat's session and
    /// project.
    async fn add_message(
        &self,
        chat: Uuid,
        seq: i64,
        role: MessageRole,
        text: &str,
        at: OffsetDateTime,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO chat_messages \
             (chat_id, seq, session_id, project_id, role, text, search_text, created_at) \
             SELECT id, $2, session_id, project_id, $3, $4, $5, $6 FROM chats WHERE id = $1 \
             ON CONFLICT (chat_id, seq) DO NOTHING",
        )
        .bind(chat.to_string())
        .bind(seq)
        .bind(role.as_str())
        .bind(text)
        .bind(text.to_lowercase())
        .bind(to_unix_micros(at))
        .execute(&self.db)
        .await?;
        Ok(())
    }
}

fn to_session(row: &AnyRow) -> Result<Chat, RepoError> {
    let id = row.uuid("id")?;
    let invalid = |what: &str| RepoError::InvalidData(format!("session {id}: {what}"));
    Ok(Chat {
        state: ChatState::parse(&row.value::<String>("state")?).ok_or_else(|| invalid("state"))?,
        status: ChatStatus::parse(&row.value::<String>("status")?)
            .ok_or_else(|| invalid("status"))?,
        // Requests wait on a live agent: written in a newer format, they are
        // dropped and the agent asks again.
        open_requests: match row.format("open_requests")? {
            OPEN_REQUESTS_FORMAT => match row.json("open_requests")? {
                Value::Array(items) => items,
                _ => Vec::new(),
            },
            _ => Vec::new(),
        },
        session_id: row.uuid("session_id")?,
        id,
        project_id: row.uuid("project_id")?,
        machine_id: row.uuid("machine_id")?,
        agent: row.value("agent")?,
        native_id: row.value("native_id")?,
        model: row.value("model")?,
        effort: row.value("effort")?,
        mode: row.value("mode")?,
        title: row.value("title")?,
        unrestricted: row.flag("unrestricted")?,
        current_turn: row.value("current_turn")?,
        close_reason: row.value("close_reason")?,
        created_at: row.time("created_at")?,
        updated_at: row.time("updated_at")?,
        closed_at: row.opt_time("closed_at")?,
    })
}

impl DbChatRepository {
    /// The chats `filter` finds, most recently changed first; `filter`
    /// compares columns with `$1`, `$2`…, bound to `values` in order.
    async fn many(&self, filter: &str, values: &[&str]) -> Result<Vec<Chat>, RepoError> {
        let sql = format!(
            "SELECT {SELECTED} FROM chats WHERE {filter} ORDER BY updated_at DESC, id DESC"
        );
        let mut query = sqlx::query(assembled(sql));
        for value in values {
            query = query.bind((*value).to_owned());
        }
        query
            .fetch_all(&self.db)
            .await?
            .iter()
            .map(to_session)
            .collect()
    }

    /// Binds `chat`'s columns as `$1`… in [`COLUMNS`]' order and runs `sql`.
    async fn write(&self, sql: String, chat: &Chat) -> Result<u64, RepoError> {
        let result = sqlx::query(assembled(sql))
            .bind(chat.id.to_string())
            .bind(chat.session_id.to_string())
            .bind(chat.project_id.to_string())
            .bind(chat.machine_id.to_string())
            .bind(chat.agent.clone())
            .bind(chat.native_id.clone())
            .bind(chat.model.clone())
            .bind(chat.effort.clone())
            .bind(chat.mode.clone())
            .bind(chat.title.clone())
            .bind(chat.state.as_str())
            .bind(chat.status.as_str())
            .bind(chat.unrestricted)
            .bind(chat.current_turn.clone())
            .bind(json_text(&chat.open_requests)?)
            .bind(chat.close_reason.clone())
            .bind(to_unix_micros(chat.created_at))
            .bind(to_unix_micros(chat.updated_at))
            .bind(chat.closed_at.map(to_unix_micros))
            .bind(OPEN_REQUESTS_FORMAT)
            .execute(&self.db)
            .await?;
        Ok(result.rows_affected())
    }
}

#[async_trait]
impl ChatRepository for DbChatRepository {
    async fn get(&self, id: Uuid) -> Result<Option<Chat>, RepoError> {
        Ok(self
            .many("id = $1", &[&id.to_string()])
            .await?
            .into_iter()
            .next())
    }

    async fn submission(&self, chat: Uuid, id: Uuid) -> Result<Option<Submission>, RepoError> {
        sqlx::query("SELECT id, fingerprint, state, queue_position, CAST(interrupting AS INTEGER) AS interrupting, \
            shown, prompt, attachments, attachments_format, command, command_format FROM chat_submissions WHERE chat_id = $1 AND id = $2")
            .bind(chat.to_string())
            .bind(id.to_string())
            .fetch_optional(&self.db)
            .await?
            .as_ref()
            .map(to_submission)
            .transpose()
    }

    async fn add_submission(&self, chat: Uuid, message: &Submission) -> Result<bool, RepoError> {
        let result = sqlx::query(
            "INSERT INTO chat_submissions \
             (chat_id, id, fingerprint, state, queue_position, interrupting, shown, prompt, attachments, attachments_format, command, command_format) \
             VALUES ($1, $2, $3, $4, \
             CASE WHEN $5 THEN (SELECT COALESCE(MAX(queue_position), 0) + 1 FROM chat_submissions WHERE chat_id = $1) ELSE NULL END, \
             $6, $7, $8, $9, $10, $11, 1) ON CONFLICT (chat_id, id) DO NOTHING",
        )
        .bind(chat.to_string()).bind(message.id.to_string()).bind(&message.fingerprint)
        .bind(message.state.as_str()).bind(message.queued).bind(message.interrupt)
        .bind(&message.shown).bind(&message.prompt).bind(json_text(&message.attachments)?)
        .bind(SUBMISSION_ATTACHMENTS_FORMAT).bind(json_text(&message.command)?)
        .execute(&self.db).await?;
        Ok(result.rows_affected() == 1)
    }

    async fn queued(&self, chat: Uuid) -> Result<Vec<Submission>, RepoError> {
        sqlx::query("SELECT id, fingerprint, state, queue_position, CAST(interrupting AS INTEGER) AS interrupting, \
             shown, prompt, attachments, attachments_format, command, command_format FROM chat_submissions WHERE chat_id = $1 \
             AND queue_position IS NOT NULL AND state IN ('queued', 'unconfirmed') AND attachments_format = $2 \
             ORDER BY interrupting DESC, queue_position, id")
            .bind(chat.to_string())
            .bind(SUBMISSION_ATTACHMENTS_FORMAT)
            .fetch_all(&self.db)
            .await?
            .iter()
            .map(to_submission)
            .collect()
    }

    async fn promote_submission(&self, chat: Uuid, id: Uuid) -> Result<bool, RepoError> {
        let result = sqlx::query(
            "UPDATE chat_submissions SET queue_position = CASE WHEN interrupting THEN queue_position \
             ELSE (SELECT COALESCE(MAX(queue_position), 0) + 1 FROM chat_submissions WHERE chat_id = $1) END, \
             interrupting = TRUE WHERE chat_id = $1 AND id = $2 AND state = 'queued' AND attachments_format = $3",
        ).bind(chat.to_string()).bind(id.to_string()).bind(SUBMISSION_ATTACHMENTS_FORMAT).execute(&self.db).await?;
        Ok(result.rows_affected() == 1)
    }

    async fn claim_submission(
        &self,
        chat: Uuid,
        id: Uuid,
        after: Option<i64>,
    ) -> Result<bool, RepoError> {
        let result = sqlx::query(
            "UPDATE chat_submissions SET state = 'sending', after_seq = $3 \
             WHERE chat_id = $1 AND id = $2 AND state = 'queued' AND attachments_format = $4 \
             AND NOT EXISTS (SELECT 1 FROM chat_submissions WHERE chat_id = $1 AND queue_position IS NOT NULL \
                 AND (state = 'sending' OR (after_seq = $3 AND state IN ('sent', 'unconfirmed'))))",
        ).bind(chat.to_string()).bind(id.to_string()).bind(after).bind(SUBMISSION_ATTACHMENTS_FORMAT).execute(&self.db).await;
        match result {
            Ok(result) => Ok(result.rows_affected() == 1),
            Err(sqlx::Error::Database(error)) if error.is_unique_violation() => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    async fn update_submission(
        &self,
        chat: Uuid,
        id: Uuid,
        before: SubmissionState,
        after: SubmissionState,
    ) -> Result<bool, RepoError> {
        let result = sqlx::query(
            "UPDATE chat_submissions SET state = $4, \
             shown = CASE WHEN $4 IN ('sent', 'withdrawn') THEN '' ELSE shown END, \
             prompt = CASE WHEN $4 IN ('sent', 'withdrawn') THEN '' ELSE prompt END, \
             command = CASE WHEN $4 IN ('sent', 'withdrawn') THEN 'null' ELSE command END,
             attachments = CASE WHEN $4 IN ('sent', 'withdrawn') THEN '[]' ELSE attachments END \
             WHERE chat_id = $1 AND id = $2 AND state = $3",
        )
        .bind(chat.to_string())
        .bind(id.to_string())
        .bind(before.as_str())
        .bind(after.as_str())
        .execute(&self.db)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn forget_submission(&self, chat: Uuid, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM chat_submissions WHERE chat_id = $1 AND id = $2 AND state = 'sending' AND queue_position IS NULL")
            .bind(chat.to_string()).bind(id.to_string()).execute(&self.db).await?;
        Ok(())
    }

    async fn withdraw_queue(&self, chat: Uuid) -> Result<bool, RepoError> {
        let result = sqlx::query("UPDATE chat_submissions SET state = 'withdrawn', shown = '', prompt = '', command = 'null', attachments = '[]' \
            WHERE chat_id = $1 AND state = 'queued'").bind(chat.to_string()).execute(&self.db).await?;
        Ok(result.rows_affected() != 0)
    }

    async fn recover_submissions(&self) -> Result<(), RepoError> {
        sqlx::query("UPDATE chat_submissions SET state = 'unconfirmed' WHERE state = 'sending'")
            .execute(&self.db)
            .await?;
        Ok(())
    }

    async fn list_for_project(&self, project: Uuid) -> Result<Vec<Chat>, RepoError> {
        self.many("project_id = $1", &[&project.to_string()]).await
    }

    async fn list_for_session(&self, session: Uuid) -> Result<Vec<Chat>, RepoError> {
        self.many("session_id = $1", &[&session.to_string()]).await
    }

    async fn defaults(&self) -> Result<Vec<Defaults>, RepoError> {
        sqlx::query(
            "SELECT agent, model, effort, mode, updated_at FROM chat_defaults \
             WHERE user_id IS NULL",
        )
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(|row| {
            Ok(Defaults {
                agent: row.value("agent")?,
                model: row.value("model")?,
                effort: row.value("effort")?,
                mode: row.value("mode")?,
                updated_at: row.time("updated_at")?,
            })
        })
        .collect()
    }

    async fn save_defaults(&self, defaults: &Defaults) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO chat_defaults (agent, user_id, model, effort, mode, updated_at) \
             VALUES ($1, NULL, $2, $3, $4, $5) \
             ON CONFLICT (agent) WHERE user_id IS NULL DO UPDATE SET model = excluded.model, \
             effort = excluded.effort, mode = excluded.mode, updated_at = excluded.updated_at",
        )
        .bind(defaults.agent.clone())
        .bind(defaults.model.clone())
        .bind(defaults.effort.clone())
        .bind(defaults.mode.clone())
        .bind(to_unix_micros(defaults.updated_at))
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn last_seq(&self, chat: Uuid) -> Result<i64, RepoError> {
        let row = sqlx::query("SELECT MAX(seq) AS seq FROM chat_events WHERE chat_id = $1")
            .bind(chat.to_string())
            .fetch_one(&self.db)
            .await?;
        Ok(row.value::<Option<i64>>("seq")?.unwrap_or(0))
    }

    async fn delete(&self, chat: Uuid) -> Result<(), RepoError> {
        let mut txn = self.db.begin().await?;
        for sql in [
            "DELETE FROM chat_messages WHERE chat_id = $1",
            "DELETE FROM chat_events WHERE chat_id = $1",
            "DELETE FROM chats WHERE id = $1",
        ] {
            sqlx::query(sql)
                .bind(chat.to_string())
                .execute(&mut *txn)
                .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    async fn fast(&self, chat: Uuid) -> Result<Option<bool>, RepoError> {
        let row = sqlx::query(
            "SELECT CAST(fast AS INTEGER) AS fast FROM chat_fast_modes WHERE chat_id = $1",
        )
        .bind(chat.to_string())
        .fetch_optional(&self.db)
        .await?;
        row.map(|row| row.value::<i64>("fast").map(|fast| fast != 0))
            .transpose()
    }

    async fn set_fast(&self, chat: Uuid, fast: Option<bool>) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM chat_fast_modes WHERE chat_id = $1")
            .bind(chat.to_string())
            .execute(&self.db)
            .await?;
        if let Some(fast) = fast {
            sqlx::query("INSERT INTO chat_fast_modes (chat_id, fast) VALUES ($1, $2)")
                .bind(chat.to_string())
                .bind(fast)
                .execute(&self.db)
                .await?;
        }
        Ok(())
    }

    async fn move_to(&self, chat: Uuid, session: Uuid) -> Result<(), RepoError> {
        let mut txn = self.db.begin().await?;
        for sql in [
            "UPDATE chats SET session_id = $2 WHERE id = $1",
            "UPDATE chat_messages SET session_id = $2 WHERE chat_id = $1",
        ] {
            sqlx::query(sql)
                .bind(chat.to_string())
                .bind(session.to_string())
                .execute(&mut *txn)
                .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    async fn delete_for_session(&self, session: Uuid) -> Result<(), RepoError> {
        let mut txn = self.db.begin().await?;
        sqlx::query(
            "DELETE FROM chat_events WHERE chat_id IN (SELECT id FROM chats WHERE session_id = $1)",
        )
        .bind(session.to_string())
        .execute(&mut *txn)
        .await?;
        sqlx::query("DELETE FROM chats WHERE session_id = $1")
            .bind(session.to_string())
            .execute(&mut *txn)
            .await?;
        txn.commit().await?;
        Ok(())
    }

    async fn live_on_machine(&self, machine: Uuid) -> Result<Vec<Chat>, RepoError> {
        self.many(
            "machine_id = $1 AND state = $2",
            &[&machine.to_string(), ChatState::Live.as_str()],
        )
        .await
    }

    async fn working(&self) -> Result<u64, RepoError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chats WHERE state = $1 AND (status = $2 OR status = $3)",
        )
        .bind(ChatState::Live.as_str())
        .bind(ChatStatus::Working.as_str())
        .bind(ChatStatus::Starting.as_str())
        .fetch_one(&self.db)
        .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    async fn waiting(&self) -> Result<Vec<Chat>, RepoError> {
        // Few sessions are live; filtering their requests here keeps the
        // query portable (no JSON operators).
        Ok(self
            .many("state = $1", &[ChatState::Live.as_str()])
            .await?
            .into_iter()
            .filter(|session| !session.open_requests.is_empty())
            .collect())
    }

    async fn save_fork(
        &self,
        source: &Chat,
        native: &str,
        seq: i64,
        new_session: bool,
        now: OffsetDateTime,
    ) -> Result<Uuid, RepoError> {
        let mut tx = self.db.begin().await?;
        let now = to_unix_micros(now);
        let chat = Uuid::now_v7();
        let title = format!("Fork of {}", source.title);
        let session = if new_session {
            let session = Uuid::now_v7();
            sqlx::query(
                "INSERT INTO sessions (id, project_id, kind, title, folder_id, created_at, updated_at)
                    SELECT $1, project_id, 'chat', $2, folder_id, $3, $3 FROM sessions WHERE id = $4",
            )
            .bind(session.to_string())
            .bind(&title)
            .bind(now)
            .bind(source.session_id.to_string())
            .execute(&mut *tx)
            .await?;
            session
        } else {
            source.session_id
        };
        let inserted = sqlx::query(
            "INSERT INTO chats (id, session_id, project_id, machine_id, agent, native_id, model, effort, mode, title, state, status, unrestricted, open_requests, created_at, updated_at, closed_at)
            SELECT $1, $2, project_id, machine_id, agent, $3, model, effort, mode, $4,
                    'closed', 'idle', unrestricted, '[]', $5, $5, $5 FROM chats WHERE id = $6
                    AND EXISTS (SELECT 1 FROM sessions WHERE id = $2 AND archived_at IS NULL)",
        )
        .bind(chat.to_string())
        .bind(session.to_string())
        .bind(native)
        .bind(title)
        .bind(now)
        .bind(source.id.to_string())
        .execute(&mut *tx)
        .await?;
        changed_one(inserted.rows_affected(), source.id)?;
        sqlx::query(
            "INSERT INTO chat_events (chat_id, seq, kind, data, data_format, created_at) SELECT $1, seq, kind, data, data_format, created_at FROM chat_events WHERE chat_id = $2 AND seq <= $3",
        )
        .bind(chat.to_string())
        .bind(source.id.to_string())
        .bind(seq)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO chat_messages (chat_id, seq, session_id, project_id, role, text, search_text, created_at) SELECT $1, seq, $2, project_id, role, text, search_text, created_at FROM chat_messages WHERE chat_id = $3 AND seq <= $4",
        )
        .bind(chat.to_string())
        .bind(session.to_string())
        .bind(source.id.to_string())
        .bind(seq)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO chat_fast_modes (chat_id, fast) SELECT $1, fast FROM chat_fast_modes WHERE chat_id = $2",
        )
        .bind(chat.to_string())
        .bind(source.id.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(chat)
    }

    async fn insert(&self, session: &Chat) -> Result<(), RepoError> {
        self.write(
            format!(
                "INSERT INTO chats ({COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, \
                 $12, $13, $14, $15, $16, $17, $18, $19, $20)"
            ),
            session,
        )
        .await?;
        Ok(())
    }

    async fn update_activity(&self, chat: &Chat) -> Result<(), RepoError> {
        let changed = sqlx::query(
            "UPDATE chats SET state = $2, status = $3, current_turn = $4, open_requests = $5, \
             open_requests_format = $6, close_reason = $7, updated_at = $8, closed_at = $9 \
             WHERE id = $1",
        )
        .bind(chat.id.to_string())
        .bind(chat.state.as_str())
        .bind(chat.status.as_str())
        .bind(chat.current_turn.clone())
        .bind(json_text(&chat.open_requests)?)
        .bind(OPEN_REQUESTS_FORMAT)
        .bind(chat.close_reason.clone())
        .bind(to_unix_micros(chat.updated_at))
        .bind(chat.closed_at.map(to_unix_micros))
        .execute(&self.db)
        .await?
        .rows_affected();
        changed_one(changed, format_args!("chat {}", chat.id))
    }

    async fn record_started(&self, chat: &Chat) -> Result<(), RepoError> {
        let changed = sqlx::query(
            "UPDATE chats SET native_id = $2, unrestricted = $3, model = $4, effort = $5, \
             status = CASE WHEN status = $6 THEN $7 ELSE status END, updated_at = $8, \
             mode = $9 WHERE id = $1",
        )
        .bind(chat.id.to_string())
        .bind(chat.native_id.clone())
        .bind(chat.unrestricted)
        .bind(chat.model.clone())
        .bind(chat.effort.clone())
        .bind(ChatStatus::Starting.as_str())
        .bind(ChatStatus::Working.as_str())
        .bind(to_unix_micros(chat.updated_at))
        .bind(chat.mode.clone())
        .execute(&self.db)
        .await?
        .rows_affected();
        changed_one(changed, format_args!("chat {}", chat.id))
    }

    async fn user_messages(&self, chat: Uuid) -> Result<Vec<(i64, String)>, RepoError> {
        let rows = sqlx::query(
            "SELECT seq, text FROM chat_messages WHERE chat_id = $1 AND role = $2 ORDER BY seq",
        )
        .bind(chat.to_string())
        .bind(MessageRole::User.as_str())
        .fetch_all(&self.db)
        .await?;
        rows.iter()
            .map(|row| Ok((row.value("seq")?, row.value("text")?)))
            .collect()
    }

    async fn reorder_queue(&self, chat: Uuid, order: &[Uuid]) -> Result<(), RepoError> {
        let mut transaction = self.db.begin().await?;
        for (position, id) in (1_i64..).zip(order) {
            sqlx::query(
                "UPDATE chat_submissions SET queue_position = $3 \
                 WHERE chat_id = $1 AND id = $2 AND state = 'queued' AND queue_position IS NOT NULL",
            )
            .bind(chat.to_string())
            .bind(id.to_string())
            .bind(position)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn edit_queued(
        &self,
        chat: Uuid,
        id: Uuid,
        shown: &str,
        prompt: &str,
    ) -> Result<bool, RepoError> {
        let result = sqlx::query(
            "UPDATE chat_submissions SET shown = $3, prompt = $4 \
             WHERE chat_id = $1 AND id = $2 AND state = 'queued' AND queue_position IS NOT NULL",
        )
        .bind(chat.to_string())
        .bind(id.to_string())
        .bind(shown)
        .bind(prompt)
        .execute(&self.db)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn mark_named(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("INSERT INTO named_chats (chat_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        Ok(())
    }

    async fn is_named(&self, id: Uuid) -> Result<bool, RepoError> {
        Ok(sqlx::query("SELECT 1 FROM named_chats WHERE chat_id = $1")
            .bind(id.to_string())
            .fetch_optional(&self.db)
            .await?
            .is_some())
    }

    async fn update(&self, session: &Chat) -> Result<(), RepoError> {
        let changed = self
            .write(
                "UPDATE chats SET session_id = $2, project_id = $3, \
                 machine_id = $4, agent = $5, native_id = $6, model = $7, effort = $8, \
                 mode = $9, title = $10, state = $11, status = $12, unrestricted = $13, \
                 current_turn = $14, open_requests = $15, close_reason = $16, created_at = $17, \
                 updated_at = $18, closed_at = $19, open_requests_format = $20 WHERE id = $1"
                    .to_owned(),
                session,
            )
            .await?;
        changed_one(changed, format_args!("chat {}", session.id))
    }

    async fn add_event(
        &self,
        session: Uuid,
        seq: i64,
        event: &Value,
        at: OffsetDateTime,
    ) -> Result<bool, RepoError> {
        let kind = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let result = sqlx::query(
            "INSERT INTO chat_events (chat_id, seq, kind, data, data_format, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (chat_id, seq) DO NOTHING",
        )
        .bind(session.to_string())
        .bind(seq)
        .bind(kind)
        .bind(json_text(event)?)
        .bind(EVENT_FORMAT)
        .bind(to_unix_micros(at))
        .execute(&self.db)
        .await?;
        let new = result.rows_affected() > 0;
        if new && let Some((role, text)) = message_of(event) {
            self.add_message(session, seq, role, &text, at).await?;
        }
        Ok(new)
    }

    async fn search_messages(
        &self,
        project: Uuid,
        terms: &[String],
        limit: u64,
    ) -> Result<Vec<MessageHit>, RepoError> {
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT m.chat_id, m.seq, m.session_id, m.role, m.text, m.created_at, \
             s.title AS session_title, s.project_id \
             FROM chat_messages m JOIN sessions s ON s.id = m.session_id \
             WHERE m.project_id = $1 AND {} \
             ORDER BY m.created_at DESC, m.seq DESC, m.chat_id LIMIT {}",
            all_terms("m.search_text", terms.len(), 2),
            limit.min(200),
        );
        let mut query = sqlx::query(assembled(sql)).bind(project.to_string());
        for term in terms {
            query = query.bind(contains_pattern(term));
        }
        query
            .fetch_all(&self.db)
            .await?
            .iter()
            .map(|row| {
                Ok(MessageHit {
                    chat_id: row.uuid("chat_id")?,
                    seq: row.value("seq")?,
                    session_id: row.uuid("session_id")?,
                    project_id: row.uuid("project_id")?,
                    session_title: row.value("session_title")?,
                    role: MessageRole::parse(&row.value::<String>("role")?)
                        .ok_or_else(|| RepoError::InvalidData("chat message: role".to_owned()))?,
                    text: row.value("text")?,
                    created_at: row.time("created_at")?,
                })
            })
            .collect()
    }

    async fn search_titles(
        &self,
        project: Uuid,
        terms: &[String],
        limit: u64,
    ) -> Result<Vec<(Uuid, Uuid, String, OffsetDateTime)>, RepoError> {
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT id, project_id, title, updated_at FROM sessions \
             WHERE project_id = $1 AND {} ORDER BY updated_at DESC, id DESC LIMIT {}",
            all_terms("LOWER(title)", terms.len(), 2),
            limit.min(200),
        );
        let mut query = sqlx::query(assembled(sql)).bind(project.to_string());
        for term in terms {
            query = query.bind(contains_pattern(term));
        }
        query
            .fetch_all(&self.db)
            .await?
            .iter()
            .map(|row| {
                Ok((
                    row.uuid("id")?,
                    row.uuid("project_id")?,
                    row.value("title")?,
                    row.time("updated_at")?,
                ))
            })
            .collect()
    }

    async fn index_messages(&self) -> Result<u64, RepoError> {
        let already: i64 = sqlx::query("SELECT COUNT(*) AS n FROM chat_messages")
            .fetch_one(&self.db)
            .await?
            .value("n")?;
        if already > 0 {
            return Ok(0);
        }
        let rows = sqlx::query(
            "SELECT chat_id, seq, data, created_at FROM chat_events \
             WHERE kind = $1 AND data_format = $2 ORDER BY created_at",
        )
        .bind("itemEnd")
        .bind(EVENT_FORMAT)
        .fetch_all(&self.db)
        .await?;
        let mut taken = 0;
        for row in &rows {
            let event: Value = row.json("data")?;
            if let Some((role, text)) = message_of(&event) {
                self.add_message(
                    row.uuid("chat_id")?,
                    row.value("seq")?,
                    role,
                    &text,
                    row.time("created_at")?,
                )
                .await?;
                taken += 1;
            }
        }
        Ok(taken)
    }

    async fn finished_items(&self, session: Uuid) -> Result<Vec<Value>, RepoError> {
        sqlx::query(
            "SELECT data FROM chat_events \
             WHERE chat_id IN (SELECT id FROM chats WHERE session_id = $1) AND kind = $2 \
             AND data_format = $3 ORDER BY created_at, chat_id, seq",
        )
        .bind(session.to_string())
        .bind("itemEnd")
        .bind(EVENT_FORMAT)
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(|row| row.json("data"))
        .collect()
    }

    async fn events_of_kind(&self, chat: Uuid, kind: &str) -> Result<Vec<Value>, RepoError> {
        sqlx::query(
            "SELECT data FROM chat_events WHERE chat_id = $1 AND kind = $2 \
             AND data_format = $3 ORDER BY seq",
        )
        .bind(chat.to_string())
        .bind(kind)
        .bind(EVENT_FORMAT)
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(|row| row.json("data"))
        .collect()
    }

    async fn events(
        &self,
        session: Uuid,
        after: i64,
        limit: u64,
    ) -> Result<Vec<StoredEvent>, RepoError> {
        sqlx::query(
            "SELECT seq, data, data_format, created_at FROM chat_events \
             WHERE chat_id = $1 AND seq > $2 ORDER BY seq LIMIT $3",
        )
        .bind(session.to_string())
        .bind(after)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(stored_event)
        .collect()
    }

    async fn events_before(
        &self,
        chat: Uuid,
        before: i64,
        limit: u64,
    ) -> Result<Vec<StoredEvent>, RepoError> {
        let rows = sqlx::query(
            "SELECT seq, data, data_format, created_at FROM chat_events \
             WHERE chat_id = $1 AND seq < $2 ORDER BY seq DESC LIMIT $3",
        )
        .bind(chat.to_string())
        .bind(before)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.db)
        .await?;
        rows.iter().map(stored_event).collect()
    }
}

fn stored_event(row: &sqlx::any::AnyRow) -> Result<StoredEvent, RepoError> {
    Ok(StoredEvent {
        seq: row.value("seq")?,
        event: match row.format("data")? {
            EVENT_FORMAT => row.json("data")?,
            format => json!({
                "type": "unreadable",
                "format": format,
                "data": row.value::<String>("data")?,
            }),
        },
        created_at: row.time("created_at")?,
    })
}

/// Each agent's models on each machine, as last read.
#[async_trait]
pub trait ModelCacheRepository: Send + Sync {
    /// The models (Stubb's catalog as Ahab keeps it) and when they were read.
    async fn get(
        &self,
        machine: Uuid,
        agent: &str,
    ) -> Result<Option<(Value, OffsetDateTime)>, RepoError>;
    async fn put(
        &self,
        machine: Uuid,
        agent: &str,
        models: &Value,
        fetched_at: OffsetDateTime,
    ) -> Result<(), RepoError>;
}

pub struct DbModelCacheRepository {
    db: Db,
}

impl DbModelCacheRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ModelCacheRepository for DbModelCacheRepository {
    async fn get(
        &self,
        machine: Uuid,
        agent: &str,
    ) -> Result<Option<(Value, OffsetDateTime)>, RepoError> {
        let Some(row) = sqlx::query(
            "SELECT models, models_format, fetched_at FROM agent_models \
             WHERE machine_id = $1 AND agent = $2",
        )
        .bind(machine.to_string())
        .bind(agent.to_owned())
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
        // A cache: written in a newer format, the models are read again.
        if row.format("models")? != MODELS_FORMAT {
            return Ok(None);
        }
        let value = serde_json::from_str(&row.value::<String>("models")?)
            .map_err(|error| RepoError::InvalidData(format!("models of {agent}: {error}")))?;
        Ok(Some((value, row.time("fetched_at")?)))
    }

    async fn put(
        &self,
        machine: Uuid,
        agent: &str,
        models: &Value,
        fetched_at: OffsetDateTime,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO agent_models (machine_id, agent, models, models_format, fetched_at) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT (machine_id, agent) \
             DO UPDATE SET models = excluded.models, models_format = excluded.models_format, \
             fetched_at = excluded.fetched_at",
        )
        .bind(machine.to_string())
        .bind(agent.to_owned())
        .bind(models.to_string())
        .bind(MODELS_FORMAT)
        .bind(to_unix_micros(fetched_at))
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
