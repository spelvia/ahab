//! The projects API's request and response types.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::projects::project::model::{Availability, Project, ProjectMode, ProjectState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ProjectState)]
pub enum ProjectStateDto {
    Active,
    /// Hidden; everything is kept.
    Archived,
}

impl From<ProjectState> for ProjectStateDto {
    fn from(state: ProjectState) -> Self {
        match state {
            ProjectState::Active => Self::Active,
            ProjectState::Archived => Self::Archived,
        }
    }
}

impl From<ProjectStateDto> for ProjectState {
    fn from(state: ProjectStateDto) -> Self {
        match state {
            ProjectStateDto::Active => Self::Active,
            ProjectStateDto::Archived => Self::Archived,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ProjectMode)]
pub enum ProjectModeDto {
    ReadWrite,
    /// Agents and tools may only read its files.
    ReadOnly,
}

impl From<ProjectMode> for ProjectModeDto {
    fn from(mode: ProjectMode) -> Self {
        match mode {
            ProjectMode::ReadWrite => Self::ReadWrite,
            ProjectMode::ReadOnly => Self::ReadOnly,
        }
    }
}

impl From<ProjectModeDto> for ProjectMode {
    fn from(mode: ProjectModeDto) -> Self {
        match mode {
            ProjectModeDto::ReadWrite => Self::ReadWrite,
            ProjectModeDto::ReadOnly => Self::ReadOnly,
        }
    }
}

/// Whether a project can be used right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ProjectAvailability)]
pub enum AvailabilityDto {
    Available,
    MachineOffline,
    Missing,
}

impl From<Availability> for AvailabilityDto {
    fn from(availability: Availability) -> Self {
        match availability {
            Availability::Available => Self::Available,
            Availability::MachineOffline => Self::MachineOffline,
            Availability::Missing => Self::Missing,
        }
    }
}

/// What you work on: one folder on one machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Project)]
pub struct ProjectDto {
    pub id: Uuid,
    #[schema(example = "ahab")]
    pub name: String,
    pub description: Option<String>,
    pub state: ProjectStateDto,
    pub machine_id: Uuid,
    #[schema(example = "atlas")]
    pub machine_name: String,
    /// The folder's absolute path on its machine.
    #[schema(example = "/home/alex/projects/ahab")]
    pub path: String,
    pub mode: ProjectModeDto,
    /// The heading it is listed under; absent: with the ungrouped ones.
    #[schema(example = "spelvia")]
    pub group: Option<String>,
    pub availability: AvailabilityDto,
    /// Its own mark instead of its initial; absent for its initial.
    pub icon: Option<ProjectIconDto>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl ProjectDto {
    pub fn new(project: &Project, machine_name: String, availability: Availability) -> Self {
        Self {
            id: project.id,
            name: project.name.to_string(),
            description: project.description.clone(),
            state: project.state.into(),
            machine_id: project.machine_id,
            machine_name,
            path: project.path.clone(),
            mode: project.mode.into(),
            group: project.group.clone(),
            availability: availability.into(),
            icon: ProjectIconDto::of(project),
            created_at: project.created_at,
            updated_at: project.updated_at,
        }
    }
}

/// A project's own mark: an emoji, or an image to fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ProjectIcon)]
pub struct ProjectIconDto {
    pub emoji: Option<String>,
    /// The image's address (it changes with the image).
    pub image: Option<String>,
}

impl ProjectIconDto {
    fn of(project: &Project) -> Option<Self> {
        let look = &project.look;
        if look.emoji.is_none() && !look.image {
            return None;
        }
        let version = look
            .icon_at
            .map_or(0, |at| at.unix_timestamp_nanos() / 1000);
        Some(Self {
            emoji: look.emoji.clone(),
            image: look
                .image
                .then(|| format!("/api/v1/projects/{}/icon?v={version}", project.id)),
        })
    }
}

/// A project's new mark: an emoji, or an image as a `data:` URL (PNG,
/// JPEG, WebP or GIF, at most 256 KiB).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SetProjectIcon)]
pub struct SetProjectIconDto {
    pub emoji: Option<String>,
    #[schema(example = "data:image/png;base64,iVBORw0KGgo…")]
    pub image: Option<String>,
}

/// The order projects are listed in, by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = ProjectOrder)]
pub struct ProjectOrderDto {
    /// Projects' IDs, first first; projects left out come after, by name.
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = ProjectList)]
pub struct ProjectListDto {
    pub items: Vec<ProjectDto>,
}

/// A new project: a folder on a machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CreateProject)]
pub struct CreateProjectDto {
    /// Absent: the folder's name.
    #[schema(example = "ahab")]
    pub name: Option<String>,
    pub description: Option<String>,
    /// The machine's name or ID.
    #[schema(example = "atlas")]
    pub machine: String,
    /// The folder's absolute path on that machine.
    #[schema(example = "/home/alex/projects/ahab")]
    pub path: String,
    /// Absent: read-write.
    pub mode: Option<ProjectModeDto>,
    pub group: Option<String>,
}

/// Changes to a project; a blank description or group removes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = UpdateProject)]
pub struct UpdateProjectDto {
    pub name: Option<String>,
    pub description: Option<String>,
    /// `archived` archives it; `active` restores it.
    pub state: Option<ProjectStateDto>,
    pub mode: Option<ProjectModeDto>,
    pub group: Option<String>,
}

/// A folder to open: its project, or a new project for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = OpenFolder)]
pub struct OpenFolderDto {
    /// The machine's name or ID; or else `machineFingerprint`.
    pub machine: Option<String>,
    /// The machine key's fingerprint, as `ahab open` sends it.
    pub machine_fingerprint: Option<String>,
    #[schema(example = "/home/alex/projects/ahab")]
    pub path: String,
    /// When the folder is several projects: which one.
    pub project: Option<String>,
}

/// What opening a folder found or made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = OpenedFolder)]
pub struct OpenedFolderDto {
    pub project: ProjectDto,
    /// A new project was created.
    pub created: bool,
}
