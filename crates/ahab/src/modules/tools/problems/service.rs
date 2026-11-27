//! Problems on the server: the latest snapshot per project (from daemons'
//! `tools.diagnostics` events and check runs), project totals, running
//! checks, and what each agent turn changed.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::modules::machines::{MachineEvent, Presence};
use crate::modules::projects::ProjectService;
use crate::modules::sessions::{ChatService, edited_files};
use crate::modules::tools::activity::api::ActivityEvent;
use crate::modules::tools::activity::service::RepositoryActivityService;
use crate::modules::tools::problems::api::{
    CheckAnswer, CheckRequest, CheckRootsDto, CheckRunDto, ChecksDto, CountsDto, DiagnosticsEvent,
    ProblemDto, ProblemListDto, ProblemSummaryDto, ProblemsQuery, TurnDiagnosticsDto, TurnFileDto,
};
use crate::modules::tools::problems::model::{Counts, Snapshot, difference, severity_of};
use crate::modules::tools::problems::store::ProblemRepository;
use crate::modules::tools::registry::service::ToolService;
use crate::modules::tools::setting::model::ToolSettings;
use crate::modules::tools::setting::service::ToolSettingService;
use crate::modules::tools::setting::store::Owner;
use crate::shared::error::ServiceError;
use crate::shared::sync::lock;
use crate::shared::time::Clock;

/// How long checks may take (a cold `cargo clippy`).
const CHECK_TIMEOUT: Duration = Duration::from_mins(15);
/// After a turn, language servers get this long to see the changes.
const SETTLE: Duration = Duration::from_secs(3);

pub struct ProblemService {
    repo: Arc<dyn ProblemRepository>,
    tools: Arc<ToolService>,
    settings: Arc<ToolSettingService>,
    activity: Arc<RepositoryActivityService>,
    projects: Arc<ProjectService>,
    chats: Arc<ChatService>,
    presence: Arc<Presence>,
    clock: Arc<dyn Clock>,
    /// Each running turn's problems when it started, by chat.
    turns: Mutex<HashMap<Uuid, Turn>>,
}

/// A running turn: its problems when it started, and the files it changed.
struct Turn {
    id: String,
    before: Snapshot,
    changed: BTreeSet<String>,
}

/// The method that says a daemon scopes checks (to files, to checkouts).
const SCOPED: &str = "tools.checkRoots";

impl ProblemService {
    #[expect(clippy::too_many_arguments, reason = "the services it brings together")]
    pub fn new(
        repo: Arc<dyn ProblemRepository>,
        tools: Arc<ToolService>,
        settings: Arc<ToolSettingService>,
        activity: Arc<RepositoryActivityService>,
        projects: Arc<ProjectService>,
        chats: Arc<ChatService>,
        presence: Arc<Presence>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            tools,
            settings,
            activity,
            projects,
            chats,
            presence,
            clock,
            turns: Mutex::new(HashMap::new()),
        }
    }

    /// Stores a daemon's diagnostics, when they are for a project on it.
    pub async fn receive(
        &self,
        machine: Uuid,
        event: DiagnosticsEvent,
    ) -> Result<(), ServiceError> {
        let project = self.projects.get(event.project).await?;
        if project.machine_id != machine {
            return Ok(());
        }
        let mut event = event;
        // A source the project turned off keeps nothing.
        if self
            .project_settings(project.id)
            .await?
            .is_off(&event.source)
        {
            event.files.clear();
            event.replace_all = true;
            event.within = None;
        }
        self.store(event).await
    }

    async fn project_settings(&self, project: Uuid) -> Result<ToolSettings, ServiceError> {
        self.settings.get(Owner::Project(project)).await
    }

    async fn store(&self, event: DiagnosticsEvent) -> Result<(), ServiceError> {
        self.repo
            .replace(
                event.project,
                &event.source,
                &event.files,
                event.replace_all,
                event.within.as_deref(),
                self.clock.now(),
            )
            .await?;
        self.presence.bus().publish(
            "problems.changed",
            json!({ "project": event.project, "source": event.source }),
        );
        Ok(())
    }

    /// A project's problems, filtered.
    pub async fn list(
        &self,
        project: Uuid,
        query: &ProblemsQuery,
    ) -> Result<ProblemListDto, ServiceError> {
        let found = self.projects.get(project).await?;
        let off = self.project_settings(found.id).await?;
        let (mut snapshot, updated_at) = self.repo.snapshot(project).await?;
        snapshot.retain(|(_, source), _| !off.is_off(source));
        let lowest = query.severity.as_deref().and_then(severity_of).unwrap_or(4);
        let text = query.text.as_deref().map(str::to_lowercase);
        let mut counts = Counts::default();
        let mut items = Vec::new();
        for ((path, source), diagnostics) in &snapshot {
            for diagnostic in diagnostics {
                counts.add(diagnostic.severity());
                if diagnostic.severity() > lowest
                    || query.source.as_ref().is_some_and(|wanted| wanted != source)
                    || query.path.as_ref().is_some_and(|wanted| {
                        path != wanted
                            && !path.starts_with(&format!("{}/", wanted.trim_end_matches('/')))
                    })
                {
                    continue;
                }
                let problem = ProblemDto::new(path, source, diagnostic);
                if let Some(text) = &text {
                    let haystack = format!(
                        "{} {} {}",
                        problem.message,
                        problem.code.as_deref().unwrap_or_default(),
                        problem.path
                    )
                    .to_lowercase();
                    if !haystack.contains(text) {
                        continue;
                    }
                }
                items.push(problem);
            }
        }
        items.sort_by(|a, b| {
            (&a.path, a.severity as u8, a.line, a.column).cmp(&(
                &b.path,
                b.severity as u8,
                b.line,
                b.column,
            ))
        });
        Ok(ProblemListDto {
            items,
            counts: counts.into(),
            updated_at,
        })
    }

    /// A project's totals.
    pub async fn summary(&self, project: Uuid) -> Result<ProblemSummaryDto, ServiceError> {
        self.projects.get(project).await?;
        let off = self.project_settings(project).await?;
        let (mut snapshot, _) = self.repo.snapshot(project).await?;
        snapshot.retain(|(_, source), _| !off.is_off(source));
        Ok(ProblemSummaryDto {
            total: Counts::of(snapshot.values().flatten()).into(),
        })
    }

    /// Runs a project's check commands now, and stores what they found.
    pub async fn run_checks(
        &self,
        project: Uuid,
        trigger: &str,
        tools: Option<Vec<String>>,
        checkouts: Option<Vec<String>>,
        paths: Option<Vec<String>>,
    ) -> Result<ChecksDto, ServiceError> {
        let found = self.projects.get(project).await?;
        let context = self.tools.context(found.machine_id, Some(found.id)).await?;
        let request = CheckRequest {
            project,
            context,
            trigger: trigger.to_owned(),
            tools,
            paths,
            checkouts,
        };
        let answer: CheckAnswer = self
            .presence
            .ask(
                found.machine_id,
                "tools.check",
                serde_json::to_value(request)
                    .map_err(|error| ServiceError::Internal(error.to_string()))?,
                CHECK_TIMEOUT,
                |_, _| None,
                "checks_failed",
            )
            .await?;
        let mut checks = Vec::new();
        for result in answer.results {
            let counts = Counts::of(result.files.values().flatten());
            if result.ok {
                self.store(DiagnosticsEvent {
                    project,
                    source: result.tool.clone(),
                    files: result.files,
                    replace_all: true,
                    within: Some(result.root.clone()),
                })
                .await?;
            }
            checks.push(CheckRunDto {
                tool: result.tool,
                name: result.name,
                root: result.root,
                checkout: result.checkout,
                ok: result.ok,
                message: result.message,
                counts: counts.into(),
            });
        }
        let (snapshot, _) = self.repo.snapshot(project).await?;
        Ok(ChecksDto {
            checks,
            counts: Counts::of(snapshot.values().flatten()).into(),
        })
    }

    /// Where Check project would run: each check's roots, by repository
    /// or worktree.
    pub async fn check_roots(&self, project: Uuid) -> Result<CheckRootsDto, ServiceError> {
        let found = self.projects.get(project).await?;
        if !self.presence.supports(found.machine_id, SCOPED) {
            return Err(ServiceError::Conflict {
                code: "machine_outdated",
                message: "Update Ahab on this project's machine to choose where checks run."
                    .to_owned(),
            });
        }
        let context = self.tools.context(found.machine_id, Some(found.id)).await?;
        let request = CheckRequest {
            project,
            context,
            trigger: "request".to_owned(),
            tools: None,
            paths: None,
            checkouts: None,
        };
        self.presence
            .ask(
                found.machine_id,
                SCOPED,
                serde_json::to_value(request)
                    .map_err(|error| ServiceError::Internal(error.to_string()))?,
                CHECK_TIMEOUT,
                |_, _| None,
                "checks_failed",
            )
            .await
    }

    /// What a turn changed.
    pub async fn turn(&self, chat: Uuid, turn: &str) -> Result<TurnDiagnosticsDto, ServiceError> {
        self.chats.get(chat).await?;
        let Some((summary, created_at)) = self.repo.turn(chat, turn).await? else {
            return Err(ServiceError::NotFound {
                code: "turn_diagnostics_not_found",
                message: "Ahab has no problems recorded for this turn.".to_owned(),
            });
        };
        Ok(TurnDiagnosticsDto {
            chat,
            turn: turn.to_owned(),
            new: summary.new.into(),
            fixed: summary.fixed.into(),
            files: summary
                .files
                .iter()
                .map(|file| TurnFileDto {
                    path: file.path.clone(),
                    new: file
                        .new
                        .iter()
                        .map(|(source, diagnostic)| ProblemDto::new(&file.path, source, diagnostic))
                        .collect(),
                    fixed: file.fixed.into(),
                })
                .collect(),
            created_at,
        })
    }

    async fn turn_started(&self, chat: Uuid, turn: String) -> Result<(), ServiceError> {
        let found = self.chats.get(chat).await?;
        let (snapshot, _) = self.repo.snapshot(found.id).await?;
        lock(&self.turns).insert(
            chat,
            Turn {
                id: turn,
                before: snapshot,
                changed: BTreeSet::new(),
            },
        );
        Ok(())
    }

    /// Notes the files a running turn's agent changed.
    fn turn_event(&self, chat: Uuid, event: &serde_json::Value) {
        let changed = edited_files(std::slice::from_ref(event));
        if changed.is_empty() {
            return;
        }
        if let Some(turn) = lock(&self.turns).get_mut(&chat) {
            turn.changed.extend(changed);
        }
    }

    /// A turn ended: the checks that run after agents' turns run, servers
    /// settle, and the difference is kept when there is one.
    async fn turn_ended(&self, chat: Uuid) -> Result<(), ServiceError> {
        let Some(Turn {
            id: turn,
            before,
            changed,
        }) = lock(&self.turns).remove(&chat)
        else {
            return Ok(());
        };
        let found = self.chats.get(chat).await?;
        let project = self.projects.get(found.id).await?;
        let paths: Vec<String> = changed
            .iter()
            .filter_map(|path| inside(path, &project.path))
            .collect();
        if self.presence.supports(found.machine_id, SCOPED) {
            // Only the roots of the files the turn changed; none when it
            // changed none.
            if !paths.is_empty()
                && let Err(error) = self
                    .run_checks(found.id, "agentTurn", None, None, Some(paths))
                    .await
            {
                tracing::info!(%chat, %error, "checks after a turn failed");
            }
        } else if self.presence.supports(found.machine_id, "tools.check")
            && let Err(error) = self
                .run_checks(found.id, "agentTurn", None, None, None)
                .await
        {
            tracing::info!(%chat, %error, "checks after a turn failed");
        }
        tokio::time::sleep(SETTLE).await;
        let (after, _) = self.repo.snapshot(found.id).await?;
        let summary = difference(&before, &after);
        if summary.new.total() == 0 && summary.fixed.total() == 0 {
            return Ok(());
        }
        self.repo
            .put_turn(chat, &turn, &summary, self.clock.now())
            .await?;
        self.presence.bus().publish(
            "problems.turn",
            json!({ "chat": chat, "turn": turn, "project": found.id,
                    "new": CountsDto::from(summary.new), "fixed": CountsDto::from(summary.fixed) }),
        );
        Ok(())
    }

    /// Hears daemons' tool events and chats' turns for as long as the
    /// server runs.
    pub fn spawn_background(self: &Arc<Self>) {
        let mut events = self.presence.take_events_starting("tools.");
        let service = Arc::clone(self);
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                service.machine_event(event).await;
            }
        });
        let mut turns = self.chats.subscribe();
        let service = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                match turns.recv().await {
                    Ok(live) => {
                        let kind = live.event["type"].as_str().unwrap_or_default();
                        match kind {
                            "turnStart" => {
                                let turn = live.event["turnId"].as_str().map_or_else(
                                    || live.seq.unwrap_or_default().to_string(),
                                    str::to_owned,
                                );
                                if let Err(error) = service.turn_started(live.chat, turn).await {
                                    tracing::debug!(chat = %live.chat, %error, "cannot note a turn's problems");
                                }
                            }
                            "itemEnd" => {
                                service.turn_event(live.chat, &live.event);
                            }
                            "turnEnd" => {
                                let service = Arc::clone(&service);
                                let chat = live.chat;
                                tokio::spawn(async move {
                                    if let Err(error) = service.turn_ended(chat).await {
                                        tracing::info!(%chat, %error, "cannot compare a turn's problems");
                                    }
                                });
                            }
                            _ => {}
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
    }

    async fn machine_event(&self, event: MachineEvent) {
        match event.kind.as_str() {
            "tools.diagnostics" => {
                let Ok(diagnostics) = serde_json::from_value::<DiagnosticsEvent>(event.data) else {
                    return;
                };
                if let Err(error) = self.receive(event.machine, diagnostics).await {
                    tracing::debug!(machine = %event.machine, %error, "cannot store diagnostics");
                }
            }
            "tools.activity" => {
                let Ok(activity) = serde_json::from_value::<ActivityEvent>(event.data) else {
                    return;
                };
                if let Err(error) = self
                    .activity
                    .receive(
                        event.machine,
                        activity.project,
                        &activity.repository,
                        activity.worktree.as_deref(),
                    )
                    .await
                {
                    tracing::debug!(machine = %event.machine, %error, "cannot note a repository's use");
                }
            }
            "tools.job" => {
                let mut data = event.data;
                data["machine"] = json!(event.machine);
                self.presence.bus().publish("tools.job", data);
            }
            _ => {}
        }
    }

    /// Counts per file, for tabs.
    pub async fn file_counts(
        &self,
        project: Uuid,
    ) -> Result<BTreeMap<String, Counts>, ServiceError> {
        let (snapshot, _) = self.repo.snapshot(project).await?;
        let mut counts: BTreeMap<String, Counts> = BTreeMap::new();
        for ((path, _), diagnostics) in &snapshot {
            let entry = counts.entry(path.clone()).or_default();
            for diagnostic in diagnostics {
                entry.add(diagnostic.severity());
            }
        }
        Ok(counts)
    }
}

/// A path an agent gave (absolute, or relative to the project) relative
/// to the project's folder; `None` outside it.
fn inside(path: &str, folder: &str) -> Option<String> {
    let folder = folder.trim_end_matches('/');
    let relative = if path.starts_with('/') {
        path.strip_prefix(folder)?.strip_prefix('/')?
    } else {
        path
    };
    let relative = relative.trim_start_matches("./");
    (!relative.is_empty() && !relative.split('/').any(|part| part == ".."))
        .then(|| relative.to_owned())
}

#[cfg(test)]
mod tests {
    use super::inside;

    #[test]
    fn a_turns_paths_are_taken_inside_the_project() {
        assert_eq!(
            inside("/w/app/src/a.rs", "/w").as_deref(),
            Some("app/src/a.rs")
        );
        assert_eq!(inside("/w/app/a.rs", "/w/").as_deref(), Some("app/a.rs"));
        assert_eq!(inside("app/a.rs", "/w").as_deref(), Some("app/a.rs"));
        assert_eq!(inside("/wx/a.rs", "/w"), None);
        assert_eq!(inside("/elsewhere/a.rs", "/w"), None);
        assert_eq!(inside("../a.rs", "/w"), None);
    }
}
