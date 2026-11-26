//! Agent readiness per machine: asking daemons, and remembering the answers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::modules::machines::agents::api::{
    AddAgentDto, AddedToDetect, AgentCatalogDto, AgentUpdateListDto, AgentUpdatedDto,
    CatalogAgentDto, DetectRequest, Detected, UpdateAgent,
};
use crate::modules::machines::agents::model::{
    AGENTS, AddedAgent, AgentStatus, CustomLaunch, harness, kind, registry_id,
};
use crate::modules::machines::agents::store::{AddedAgentRepository, AgentRepository};
use crate::modules::machines::machine::service::MachineService;
use crate::modules::machines::presence::service::Presence;
use crate::shared::error::{FieldError, ServiceError};
use crate::shared::time::Clock;

/// How long detection may take on a machine: a few status commands.
const DETECT_TIMEOUT: Duration = Duration::from_secs(90);
/// How often connected machines are asked again.
const REDETECT_EVERY: Duration = Duration::from_mins(15);
/// How long checking for updates may take: npm, once per agent.
const UPDATES_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a machine's update check is shown before checking again.
const UPDATES_FRESH: Duration = Duration::from_hours(1);
/// How long an update may take: the agent downloads and installs itself.
const UPDATE_TIMEOUT: Duration = Duration::from_mins(6);
/// How often the ACP registry is fetched again.
const REGISTRY_EVERY: Duration = Duration::from_hours(24);

/// The ACP registry Ahab knows: the copy shipped with Stubb until one is
/// fetched.
struct Known {
    registry: stubb::providers::acp::registry::Registry,
    fetched: bool,
}

pub struct AgentService {
    repo: Arc<dyn AgentRepository>,
    added: Arc<dyn AddedAgentRepository>,
    known: RwLock<Known>,
    machines: Arc<MachineService>,
    presence: Arc<Presence>,
    clock: Arc<dyn Clock>,
    /// Each machine's last update check, and when it was made.
    updates: Mutex<HashMap<Uuid, (Instant, AgentUpdateListDto)>>,
    /// Registry agents' logos, fetched and cleaned, by URL.
    logos: Mutex<HashMap<String, String>>,
}

impl AgentService {
    pub fn new(
        repo: Arc<dyn AgentRepository>,
        added: Arc<dyn AddedAgentRepository>,
        machines: Arc<MachineService>,
        presence: Arc<Presence>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            added,
            known: RwLock::new(Known {
                registry: stubb::providers::acp::registry::Registry::bundled().clone(),
                fetched: false,
            }),
            machines,
            presence,
            clock,
            updates: Mutex::new(HashMap::new()),
            logos: Mutex::new(HashMap::new()),
        }
    }

    /// Whether newer versions of `machine`'s agents are out, checked at most
    /// hourly unless `refresh`.
    pub async fn updates(
        &self,
        machine: Uuid,
        refresh: bool,
    ) -> Result<AgentUpdateListDto, ServiceError> {
        self.machines.get(machine).await?;
        let kept = self
            .updates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&machine)
            .cloned();
        if !refresh
            && let Some((asked, list)) = &kept
            && asked.elapsed() < UPDATES_FRESH
        {
            return Ok(list.clone());
        }
        let answer = match self
            .presence
            .request(
                machine,
                "agents.updates",
                serde_json::json!({}),
                UPDATES_TIMEOUT,
            )
            .await
        {
            Ok(answer) => answer,
            Err(error) => return kept.map(|(_, list)| list).ok_or(error),
        };
        let list: AgentUpdateListDto = serde_json::from_value(answer).map_err(|error| {
            ServiceError::Internal(format!("unreadable agents.updates answer: {error}"))
        })?;
        self.updates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(machine, (Instant::now(), list.clone()));
        Ok(list)
    }

    /// Updates `agent` on `machine` with its own updater, then detects the
    /// agents again and forgets the machine's update check.
    pub async fn update(
        &self,
        machine: Uuid,
        agent: &str,
    ) -> Result<AgentUpdatedDto, ServiceError> {
        self.machines.get(machine).await?;
        if !AGENTS.contains(&agent) {
            return Err(ServiceError::NotFound {
                code: "agent_not_found",
                message: format!("Ahab does not know the agent {agent}."),
            });
        }
        let params = serde_json::to_value(UpdateAgent {
            agent: agent.to_owned(),
        })
        .map_err(|error| ServiceError::Internal(error.to_string()))?;
        let answer = self
            .presence
            .request(machine, "agents.update", params, UPDATE_TIMEOUT)
            .await?;
        let updated: AgentUpdatedDto = serde_json::from_value(answer).map_err(|error| {
            ServiceError::Internal(format!("unreadable agents.update answer: {error}"))
        })?;
        self.updates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&machine);
        if let Err(error) = self.refresh(machine).await {
            tracing::info!(%machine, %error, "cannot detect agents after an update");
        }
        Ok(updated)
    }

    /// The latest report for `machine`; empty until its daemon has answered
    /// once.
    pub async fn list(&self, machine: Uuid) -> Result<Vec<AgentStatus>, ServiceError> {
        self.machines.get(machine).await?;
        Ok(self.repo.list(machine).await?)
    }

    /// Asks `machine`'s daemon now and stores the answer.
    pub async fn refresh(&self, machine: Uuid) -> Result<Vec<AgentStatus>, ServiceError> {
        self.machines.get(machine).await?;
        let request = self.detect_request(machine).await?;
        let params = serde_json::to_value(request)
            .map_err(|error| ServiceError::Internal(error.to_string()))?;
        let answer = self
            .presence
            .request(machine, "agents.detect", params, DETECT_TIMEOUT)
            .await?;
        let detected: Detected = serde_json::from_value(answer).map_err(|error| {
            ServiceError::Internal(format!("unreadable agents.detect answer: {error}"))
        })?;
        let now = self.clock.now();
        let statuses: Vec<AgentStatus> = detected
            .agents
            .into_iter()
            .filter(|agent| kind(&agent.agent).is_some())
            .map(|agent| AgentStatus {
                machine_id: machine,
                agent: agent.agent,
                label: agent.label,
                state: agent.state.into(),
                version: agent.version,
                path: agent.path,
                account: agent.account,
                detail: agent.detail,
                checked_at: now,
            })
            .collect();
        self.repo.replace(machine, &statuses).await?;
        Ok(statuses)
    }

    /// What `agents.detect` is told about `machine`'s added agents: each
    /// registry agent's entry as the registry has it now, or the custom
    /// command.
    async fn detect_request(&self, machine: Uuid) -> Result<DetectRequest, ServiceError> {
        let added = self.added.list(machine).await?;
        let known = self.known.read().unwrap_or_else(PoisonError::into_inner);
        Ok(DetectRequest {
            added: added
                .into_iter()
                .map(|agent| AddedToDetect {
                    entry: registry_id(&agent.agent)
                        .and_then(|id| known.registry.get(id))
                        .and_then(|entry| serde_json::to_value(entry).ok()),
                    agent: agent.agent,
                    name: agent.name,
                    launch: agent.launch,
                })
                .collect(),
        })
    }

    /// Every agent Ahab can run: its own, then the ACP registry's by name.
    pub fn catalog(&self) -> AgentCatalogDto {
        let known = self.known.read().unwrap_or_else(PoisonError::into_inner);
        let builtin = AGENTS.iter().filter_map(|agent| {
            let profile = harness(agent)?.cli().profile();
            Some(CatalogAgentDto {
                id: (*agent).to_owned(),
                name: profile.name,
                maker: profile.maker,
                description: profile.description,
                has_logo: true,
                monochrome: profile.logo.monochrome,
                source: "builtin".to_owned(),
                version: None,
                repository: profile.repository,
                website: profile.homepage,
                license: None,
                methods: Vec::new(),
                addable: false,
            })
        });
        let mut registry: Vec<CatalogAgentDto> = known
            .registry
            .agents
            .iter()
            .map(|entry| {
                let mut methods = Vec::new();
                if entry.distribution.npx.is_some() {
                    methods.push("npx".to_owned());
                }
                if entry.distribution.uvx.is_some() {
                    methods.push("uvx".to_owned());
                }
                if !entry.distribution.binary.is_empty() {
                    methods.push("binary".to_owned());
                }
                let profile = entry.profile();
                CatalogAgentDto {
                    id: format!("acp:{}", entry.id),
                    name: entry.name.clone(),
                    maker: profile.maker,
                    has_logo: entry.icon.is_some(),
                    monochrome: profile.logo.monochrome,
                    description: entry.description.clone(),
                    source: "registry".to_owned(),
                    version: Some(entry.version.clone()),
                    repository: entry.repository.clone(),
                    website: entry.website.clone(),
                    license: entry.license.clone(),
                    addable: entry.distribution.npx.is_some() || entry.distribution.uvx.is_some(),
                    methods,
                }
            })
            .collect();
        registry.sort_by_key(|agent| agent.name.to_lowercase());
        AgentCatalogDto {
            items: builtin.chain(registry).collect(),
            registry_version: known.registry.version.clone(),
            registry_source: if known.fetched { "fetched" } else { "bundled" }.to_owned(),
        }
    }

    /// How to start an added agent on `machine` (`None` for Stubb's own
    /// agents), for a session: its registry entry or custom command.
    pub async fn launch_for(
        &self,
        machine: Uuid,
        agent: &str,
    ) -> Result<Option<AddedToDetect>, ServiceError> {
        if registry_id(agent).is_none() && !agent.starts_with("custom:") {
            return Ok(None);
        }
        let request = self.detect_request(machine).await?;
        let found = request.added.into_iter().find(|added| added.agent == agent);
        found.map(Some).ok_or_else(|| ServiceError::NotFound {
            code: "agent_not_found",
            message: format!("{agent} is not added to this machine."),
        })
    }

    /// An agent's logo, an SVG: Stubb's for its own harnesses, the
    /// registry's (fetched once, and only when clean) for registry agents.
    pub async fn logo(&self, agent: &str) -> Result<String, ServiceError> {
        let missing = || ServiceError::NotFound {
            code: "logo_not_found",
            message: format!("{agent} has no logo."),
        };
        if let Some(harness) = harness(agent) {
            return match harness.cli().profile().logo.source {
                stubb::discovery::profile::LogoSource::Svg(svg) => Ok(svg),
                // A URL, or a source a newer Stubb adds: none to serve here.
                _ => Err(missing()),
            };
        }
        let url = {
            let known = self.known.read().unwrap_or_else(PoisonError::into_inner);
            registry_id(agent)
                .and_then(|id| known.registry.get(id))
                .and_then(|entry| entry.icon.clone())
                .ok_or_else(missing)?
        };
        if let Some(svg) = self
            .logos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&url)
        {
            return Ok(svg.clone());
        }
        let fetched = crate::integrations::acp_registry::fetch_logo(&url)
            .await
            .map_err(|message| ServiceError::Unavailable {
                code: "logo_unavailable",
                message,
            })?;
        let clean = stubb::discovery::profile::clean_svg(&fetched).ok_or_else(|| {
            ServiceError::NotFound {
                code: "logo_not_found",
                message: format!("{agent}'s logo is not a plain SVG, so Ahab does not show it."),
            }
        })?;
        self.logos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(url, clean.clone());
        Ok(clean)
    }

    /// The agents added to `machine`.
    pub async fn added(&self, machine: Uuid) -> Result<Vec<AddedAgent>, ServiceError> {
        self.machines.get(machine).await?;
        Ok(self.added.list(machine).await?)
    }

    /// Adds an agent to `machine`: a registry one by its `acp:` id, or a
    /// custom one by name and program. Detection runs again in the background.
    pub async fn add(
        self: &Arc<Self>,
        machine: Uuid,
        request: AddAgentDto,
    ) -> Result<AddedAgent, ServiceError> {
        self.machines.get(machine).await?;
        let added = match (&request.agent, &request.program) {
            (Some(agent), _) => {
                let known = self.known.read().unwrap_or_else(PoisonError::into_inner);
                let entry = registry_id(agent)
                    .and_then(|id| known.registry.get(id))
                    .ok_or_else(|| ServiceError::NotFound {
                        code: "agent_not_found",
                        message: format!("The ACP registry has no agent {agent}."),
                    })?;
                if entry.distribution.npx.is_none() && entry.distribution.uvx.is_none() {
                    return Err(ServiceError::Conflict {
                        code: "agent_not_addable",
                        message: format!(
                            "{} comes only as a download, which Ahab cannot install yet.",
                            entry.name
                        ),
                    });
                }
                AddedAgent {
                    machine_id: machine,
                    agent: agent.clone(),
                    name: entry.name.clone(),
                    launch: None,
                    added_at: self.clock.now(),
                }
            }
            (None, Some(program)) => {
                let name = request.name.clone().unwrap_or_default();
                let slug: String = name
                    .trim()
                    .to_lowercase()
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                    .collect::<String>()
                    .split('-')
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("-");
                let agent = format!("custom:{slug}");
                if program.trim().is_empty() || kind(&agent).is_none() {
                    return Err(ServiceError::Invalid(vec![FieldError::new(
                        if program.trim().is_empty() {
                            "program"
                        } else {
                            "name"
                        },
                        "Give the agent a name of letters and digits, and the program that starts it.",
                    )]));
                }
                AddedAgent {
                    machine_id: machine,
                    agent,
                    name: name.trim().to_owned(),
                    launch: Some(CustomLaunch {
                        program: program.trim().to_owned(),
                        args: request.args,
                        env: request.env,
                    }),
                    added_at: self.clock.now(),
                }
            }
            (None, None) => {
                return Err(ServiceError::Invalid(vec![FieldError::new(
                    "agent",
                    "Name a registry agent, or a custom agent's program.",
                )]));
            }
        };
        self.added.put(&added).await?;
        self.refresh_quietly(machine);
        Ok(added)
    }

    /// Removes an added agent from `machine`; detection runs again.
    pub async fn remove(self: &Arc<Self>, machine: Uuid, agent: &str) -> Result<(), ServiceError> {
        self.machines.get(machine).await?;
        if !self.added.remove(machine, agent).await? {
            return Err(ServiceError::NotFound {
                code: "agent_not_found",
                message: format!("{agent} is not added to this machine."),
            });
        }
        self.refresh_quietly(machine);
        Ok(())
    }

    /// Fetches the ACP registry now and then; the bundled copy stands until
    /// one arrives.
    fn keep_registry(self: &Arc<Self>) {
        let service = Arc::clone(self);
        tokio::spawn(async move {
            let mut every = tokio::time::interval(REGISTRY_EVERY);
            loop {
                every.tick().await;
                match crate::integrations::acp_registry::fetch().await {
                    Ok(registry) => {
                        let mut known = service
                            .known
                            .write()
                            .unwrap_or_else(PoisonError::into_inner);
                        known.registry = registry;
                        known.fetched = true;
                    }
                    Err(error) => tracing::info!(%error, "cannot fetch the ACP registry"),
                }
            }
        });
    }

    /// Detects on every machine that connects, and on every connected machine
    /// every 15 minutes, for as long as the server runs.
    pub fn spawn_background(self: &Arc<Self>) {
        self.keep_registry();
        let service = Arc::clone(self);
        let mut connected = self.presence.subscribe();
        tokio::spawn(async move {
            let mut every = tokio::time::interval(REDETECT_EVERY);
            every.tick().await;
            loop {
                tokio::select! {
                    machine = connected.recv() => match machine {
                        Ok(machine) => service.refresh_quietly(machine),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            for machine in service.presence.online() {
                                service.refresh_quietly(machine);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    },
                    _ = every.tick() => {
                        for machine in service.presence.online() {
                            service.refresh_quietly(machine);
                        }
                    }
                }
            }
        });
    }

    pub fn refresh_quietly(self: &Arc<Self>, machine: Uuid) {
        let service = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(error) = service.refresh(machine).await {
                tracing::info!(%machine, %error, "cannot detect agents");
            }
        });
    }
}
