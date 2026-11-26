//! What can be done with projects, and keeping each machine's daemon told
//! which projects' folders it serves (`projects.sync`).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use uuid::Uuid;

use crate::modules::machines::{Machine, MachineService, MachineState, Presence};
use crate::modules::projects::files::api::{FolderCheckDto, ProjectGrant, SyncAnswer};
use crate::modules::projects::project::model::{
    self, Availability, Project, ProjectIcon, ProjectLook, ProjectMode, ProjectName, ProjectState,
};
use crate::modules::projects::project::store::ProjectRepository;
use crate::shared::error::FieldError;
use crate::shared::error::ServiceError;
use crate::shared::sync::lock;
use crate::shared::time::Clock;

const DAEMON_TIMEOUT: Duration = Duration::from_secs(20);

/// A new project: a folder on a machine.
#[derive(Debug, Clone)]
pub struct NewProject {
    /// None: the folder's name.
    pub name: Option<String>,
    pub description: Option<String>,
    /// The machine's name or ID.
    pub machine: String,
    pub path: String,
    pub mode: ProjectMode,
    pub group: Option<String>,
}

/// What may change about a project; `None` leaves a field as it is.
#[derive(Debug, Default)]
pub struct ProjectChanges {
    pub name: Option<String>,
    /// `Some(None)` clears it.
    pub description: Option<Option<String>>,
    pub state: Option<ProjectState>,
    pub mode: Option<ProjectMode>,
    /// `Some(None)` clears it.
    pub group: Option<Option<String>>,
}

/// What opening a folder found or made.
pub struct Opened {
    pub project: Project,
    /// A new project was created for it.
    pub created: bool,
}

pub struct ProjectService {
    repo: Arc<dyn ProjectRepository>,
    machines: Arc<MachineService>,
    presence: Arc<Presence>,
    clock: Arc<dyn Clock>,
    /// Projects whose machine says the folder is gone.
    missing: Mutex<HashSet<Uuid>>,
}

fn not_found() -> ServiceError {
    ServiceError::NotFound {
        code: "project_not_found",
        message: "There is no such project.".to_owned(),
    }
}

impl ProjectService {
    pub fn new(
        repo: Arc<dyn ProjectRepository>,
        machines: Arc<MachineService>,
        presence: Arc<Presence>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            machines,
            presence,
            clock,
            missing: Mutex::new(HashSet::new()),
        }
    }

    /// Active projects, or archived ones, in the order chosen by hand, then
    /// by name.
    pub async fn list(&self, state: ProjectState) -> Result<Vec<Project>, ServiceError> {
        Ok(self.repo.list(Some(state)).await?)
    }

    pub async fn get(&self, id: Uuid) -> Result<Project, ServiceError> {
        self.repo.get(id).await?.ok_or_else(not_found)
    }

    /// The project `reference` names: its ID, or the name of exactly one
    /// active project.
    pub async fn find(&self, reference: &str) -> Result<Project, ServiceError> {
        if let Ok(id) = Uuid::parse_str(reference) {
            return self.get(id).await;
        }
        let name = ProjectName::parse(reference).map_err(|_| not_found())?;
        let mut found = self.repo.find_active(&name).await?;
        match found.len() {
            0 => Err(not_found()),
            1 => Ok(found.remove(0)),
            n => Err(ServiceError::Conflict {
                code: "project_name_ambiguous",
                message: format!("{n} projects are called {name}: use the project's ID."),
            }),
        }
    }

    /// Which of a folder's projects (`existing`) `reference` names: by ID,
    /// or by a name only these projects need not share; projects elsewhere
    /// with the same name do not count.
    async fn open_named(
        &self,
        existing: Vec<Project>,
        reference: &str,
    ) -> Result<Opened, ServiceError> {
        let elsewhere = |wanted: &Project| ServiceError::Conflict {
            code: "project_elsewhere",
            message: format!(
                "{} is the folder {} on another machine or path.",
                wanted.name, wanted.path
            ),
        };
        let opened = |project| {
            Ok(Opened {
                project,
                created: false,
            })
        };
        if let Ok(id) = Uuid::parse_str(reference) {
            return match existing.into_iter().find(|p| p.id == id) {
                Some(project) => opened(project),
                None => Err(elsewhere(&self.get(id).await?)),
            };
        }
        let name = ProjectName::parse(reference).map_err(|_| not_found())?;
        let mut named: Vec<Project> = existing.into_iter().filter(|p| p.name == name).collect();
        match named.len() {
            0 => match self.repo.find_active(&name).await?.as_slice() {
                [] => Err(not_found()),
                [wanted] => Err(elsewhere(wanted)),
                _ => Err(ServiceError::Conflict {
                    code: "project_elsewhere",
                    message: format!("The projects called {name} are other folders."),
                }),
            },
            1 => opened(named.remove(0)),
            n => Err(ServiceError::Conflict {
                code: "project_name_ambiguous",
                message: format!(
                    "{n} of this folder's projects are called {name}: use the project's ID."
                ),
            }),
        }
    }

    /// Whether `project` can be used right now.
    pub fn availability(&self, project: &Project) -> Availability {
        let missing = lock(&self.missing).contains(&project.id);
        Availability::of(self.presence.is_online(project.machine_id), missing)
    }

    /// The machine a project's folder is on.
    pub async fn machine_of(&self, project: &Project) -> Result<Machine, ServiceError> {
        self.machines.get(project.machine_id).await
    }

    /// Creates a project for a folder, after its machine confirms the
    /// folder.
    pub async fn create(&self, new: NewProject) -> Result<Project, ServiceError> {
        let description = model::description(new.description.as_deref())?;
        let group = model::group(new.group.as_deref())?;
        let explicit = new.name.as_deref().map(ProjectName::parse).transpose()?;
        let (machine, real_path) = self.check(&new.machine, &new.path).await?;
        let name = match explicit {
            Some(name) => name,
            None => name_for(&real_path)?,
        };
        let project = self
            .insert(name, description, &machine, real_path, new.mode, group)
            .await?;
        self.sync(machine.id).await;
        Ok(project)
    }

    /// A folder's project: the one this folder is, else a new project
    /// named after the folder. With several projects, `project` says
    /// which.
    pub async fn open(
        &self,
        machine: &str,
        path: &str,
        project: Option<&str>,
    ) -> Result<Opened, ServiceError> {
        let (machine, real_path) = self.check(machine, path).await?;
        let mut existing: Vec<Project> = self
            .repo
            .find_folder(machine.id, &real_path)
            .await?
            .into_iter()
            .filter(|project| project.state == ProjectState::Active)
            .collect();
        if let Some(reference) = project {
            return self.open_named(existing, reference).await;
        }
        match existing.len() {
            0 => {}
            1 => {
                return Ok(Opened {
                    project: existing.remove(0),
                    created: false,
                });
            }
            _ => {
                let names: Vec<String> = existing
                    .iter()
                    .map(|p| format!("{} ({})", p.name, p.id))
                    .collect();
                return Err(ServiceError::Conflict {
                    code: "folder_in_several_projects",
                    message: format!(
                        "This folder is several projects ({}): say which.",
                        names.join(", ")
                    ),
                });
            }
        }
        let name = name_for(&real_path)?;
        let project = self
            .insert(
                name,
                None,
                &machine,
                real_path,
                ProjectMode::ReadWrite,
                None,
            )
            .await?;
        self.sync(machine.id).await;
        Ok(Opened {
            project,
            created: true,
        })
    }

    pub async fn update(&self, id: Uuid, changes: ProjectChanges) -> Result<Project, ServiceError> {
        let mut project = self.get(id).await?;
        let mode_changed = changes.mode.is_some_and(|mode| mode != project.mode);
        if let Some(name) = changes.name {
            project.name = ProjectName::parse(&name)?;
        }
        if let Some(description) = changes.description {
            project.description = model::description(description.as_deref())?;
        }
        if let Some(state) = changes.state {
            project.state = state;
        }
        if let Some(mode) = changes.mode {
            project.mode = mode;
        }
        if let Some(group) = changes.group {
            project.group = model::group(group.as_deref())?;
        }
        project.updated_at = self.clock.now();
        self.repo.update(&project).await?;
        if mode_changed {
            self.sync(project.machine_id).await;
        }
        Ok(project)
    }

    /// Orders the projects by hand: `ids` in the order given, each once;
    /// projects left out come after them, by name.
    pub async fn set_order(&self, ids: &[Uuid]) -> Result<(), ServiceError> {
        let mut seen = HashSet::new();
        for id in ids {
            if !seen.insert(*id) {
                return Err(ServiceError::Invalid(vec![FieldError::new(
                    "ids",
                    "Name each project once.",
                )]));
            }
            self.get(*id).await?;
        }
        Ok(self.repo.set_order(ids).await?)
    }

    /// Gives a project its own mark, or (none) its initial back.
    pub async fn set_icon(
        &self,
        id: Uuid,
        icon: Option<ProjectIcon>,
    ) -> Result<Project, ServiceError> {
        self.get(id).await?;
        self.repo
            .set_icon(id, icon.as_ref(), self.clock.now())
            .await?;
        self.get(id).await
    }

    /// A project's image mark: its type and bytes.
    pub async fn icon_image(&self, id: Uuid) -> Result<(String, Vec<u8>), ServiceError> {
        self.repo
            .icon_image(id)
            .await?
            .ok_or_else(|| ServiceError::NotFound {
                code: "icon_not_found",
                message: "This project has no image.".to_owned(),
            })
    }

    /// Deletes the project and its records, and tells its machine. Files
    /// are never touched.
    pub async fn delete(&self, id: Uuid) -> Result<(), ServiceError> {
        let project = self.get(id).await?;
        if !self.repo.delete(id).await? {
            return Err(not_found());
        }
        self.sync(project.machine_id).await;
        Ok(())
    }

    /// Tells `machine`'s daemon its projects' folders, and notes which are
    /// gone. Quietly does nothing when the machine is offline: it gets the
    /// list when it connects.
    pub async fn sync(&self, machine: Uuid) {
        if !self.presence.is_online(machine) {
            return;
        }
        let projects = match self.repo.list_for_machine(machine).await {
            Ok(projects) => projects,
            Err(error) => {
                tracing::warn!(%machine, %error, "cannot list the machine's projects");
                return;
            }
        };
        let grants: Vec<ProjectGrant> = projects
            .iter()
            .map(|project| ProjectGrant {
                id: project.id,
                path: project.path.clone(),
                read_only: project.mode == ProjectMode::ReadOnly,
            })
            .collect();
        let answer = self
            .presence
            .request(
                machine,
                "projects.sync",
                json!({ "projects": grants }),
                DAEMON_TIMEOUT,
            )
            .await
            .ok()
            .and_then(|value| serde_json::from_value::<SyncAnswer>(value).ok());
        let Some(answer) = answer else {
            tracing::info!(%machine, "cannot send the machine its projects");
            return;
        };
        let mut missing = lock(&self.missing);
        for project in &projects {
            missing.remove(&project.id);
        }
        missing.extend(answer.missing);
    }

    /// Syncs every machine as it connects, for as long as the server runs.
    pub fn spawn_background(self: &Arc<Self>) {
        let service = Arc::clone(self);
        let mut connected = self.presence.subscribe();
        tokio::spawn(async move {
            loop {
                match connected.recv().await {
                    Ok(machine) => {
                        let service = Arc::clone(&service);
                        tokio::spawn(async move { service.sync(machine).await });
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        for machine in service.presence.online() {
                            service.sync(machine).await;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
        });
    }

    /// The machine, and the folder's real path there, after the machine
    /// confirms it is a readable folder.
    async fn check(&self, machine: &str, path: &str) -> Result<(Machine, String), ServiceError> {
        let machine = self.machines.find(machine).await?;
        if machine.state != MachineState::Active {
            return Err(ServiceError::Conflict {
                code: "machine_pending",
                message: format!("{} waits for approval.", machine.name),
            });
        }
        let path = model::absolute_path(path)?;
        let answer = self
            .presence
            .request(
                machine.id,
                "fs.checkFolder",
                json!({ "path": path }),
                DAEMON_TIMEOUT,
            )
            .await?;
        let check: FolderCheckDto = serde_json::from_value(answer).map_err(|error| {
            ServiceError::Internal(format!("unreadable fs.checkFolder answer: {error}"))
        })?;
        if !check.exists || !check.is_dir {
            return Err(ServiceError::NotFound {
                code: "folder_not_found",
                message: format!("There is no folder {path} on {}.", machine.name),
            });
        }
        if !check.readable {
            return Err(ServiceError::Forbidden {
                code: "folder_not_readable",
                message: format!("Ahab on {} may not read {path}.", machine.name),
            });
        }
        let real = check.real_path.unwrap_or(path);
        Ok((machine, real))
    }

    async fn insert(
        &self,
        name: ProjectName,
        description: Option<String>,
        machine: &Machine,
        path: String,
        mode: ProjectMode,
        group: Option<String>,
    ) -> Result<Project, ServiceError> {
        let now = self.clock.now();
        let project = Project {
            id: Uuid::now_v7(),
            name,
            description,
            state: ProjectState::Active,
            machine_id: machine.id,
            path,
            mode,
            group,
            created_at: now,
            updated_at: now,
            look: ProjectLook::default(),
        };
        self.repo.insert(&project).await?;
        self.presence.bus().publish(
            "project.added",
            json!({
                "project": project.id,
                "machine": project.machine_id,
                "path": project.path,
            }),
        );
        Ok(project)
    }

    /// Ahab's own events.
    pub fn bus(&self) -> &Arc<crate::shared::events::Events> {
        self.presence.bus()
    }
}

/// A project's name from its folder's name.
fn name_for(path: &str) -> Result<ProjectName, ServiceError> {
    ProjectName::from_folder(path).ok_or_else(|| {
        ServiceError::Invalid(vec![FieldError::new(
            "path",
            "Choose a folder below the root.",
        )])
    })
}
