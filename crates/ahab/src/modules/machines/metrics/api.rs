//! A machine's resources on the wire: what a daemon answers to
//! `machine.metrics`, and what the API returns.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

/// The processors: how busy they are, together and each.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CpuMetrics)]
pub struct CpuDto {
    /// Busy, 0 to 100, over all logical cores.
    pub usage_percent: f32,
    /// Busy per logical core, 0 to 100.
    pub cores: Vec<f32>,
    pub physical_cores: Option<usize>,
    /// The load average over 1, 5 and 15 minutes (not on Windows).
    pub load: Option<[f64; 3]>,
}

/// Memory and swap, in bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = MemoryMetrics)]
pub struct MemoryDto {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

/// A mounted disk, in bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = DiskMetrics)]
pub struct DiskDto {
    #[schema(example = "/")]
    pub mount: String,
    #[schema(example = "ext4")]
    pub file_system: String,
    pub total: u64,
    pub available: u64,
}

/// One process. Only its program's name is sent, and the script an
/// interpreter runs, never its arguments, which may hold secrets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ProcessMetrics)]
pub struct ProcessDto {
    pub pid: u32,
    #[schema(example = "node")]
    pub name: String,
    /// The script, for interpreters such as `node` (`codex.js`).
    pub script: Option<String>,
    /// Busy, as a share of the whole machine, 0 to 100.
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    /// When it started.
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub started_at: Option<OffsetDateTime>,
    /// Started by Ahab: an agent, a terminal, a plugin's part, or Ahab itself.
    pub ahab: bool,
    /// What Ahab started it for: `ahab`, `agent:<id>`, `terminal`,
    /// `plugin:<id>`.
    pub role: Option<String>,
    pub parent: Option<u32>,
    /// The user it runs as.
    pub user: Option<String>,
    /// `Run`, `Sleep`, `Idle`, `Zombie`, `Stop`…, as the system says.
    pub status: String,
    /// How many threads it has (Linux).
    pub threads: Option<usize>,
    /// CPU time it used since it started, in milliseconds.
    pub cpu_time_ms: u64,
    /// How long it has run, in seconds.
    pub run_seconds: u64,
    pub virtual_bytes: u64,
    /// Read from and written to disk, per second, lately.
    pub read_per_second: u64,
    pub written_per_second: u64,
    /// Its program's path.
    pub executable: Option<String>,
    /// The folder it runs in.
    pub cwd: Option<String>,
}

/// The machine at one moment of the last minutes, for its charts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = MetricsSample)]
pub struct SampleDto {
    #[serde(with = "crate::shared::time::rfc3339")]
    pub at: OffsetDateTime,
    /// Busy, 0 to 100.
    pub cpu: f32,
    pub memory_used: u64,
    pub swap_used: u64,
    /// The load average over one minute (0 on Windows).
    pub load: f64,
    /// Busy for Ahab and everything it started, 0 to 100.
    pub ahab_cpu: f32,
    pub ahab_memory: u64,
}

/// Ahab's use of the machine for one purpose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AhabUse)]
pub struct AhabUseDto {
    /// `ahab` (itself), `agent`, `terminal` or `plugin`.
    #[schema(example = "agent")]
    pub kind: String,
    /// The agent (`claude-code`) or plugin (`docker`); absent otherwise.
    pub id: Option<String>,
    /// How many: agent sessions, terminals; one for a plugin or Ahab.
    pub count: usize,
    pub processes: usize,
    /// Busy, as a share of the whole machine, 0 to 100.
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

/// The processes of one program, together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AppMetrics)]
pub struct AppDto {
    #[schema(example = "docker")]
    pub name: String,
    pub processes: usize,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    /// Its busiest processes' IDs (found in `processes`).
    pub members: Vec<u32>,
}

/// A machine's resources at one moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = MachineMetrics)]
pub struct MachineMetricsDto {
    #[serde(with = "crate::shared::time::rfc3339")]
    pub sampled_at: OffsetDateTime,
    pub host_name: Option<String>,
    /// The operating system in full, such as "Linux 26.05 NixOS".
    pub os_version: Option<String>,
    pub kernel: Option<String>,
    pub uptime_seconds: u64,
    pub cpu: CpuDto,
    pub memory: MemoryDto,
    pub disks: Vec<DiskDto>,
    /// How many processes run.
    pub process_count: usize,
    /// The busiest processes (by CPU and by memory), every one Ahab
    /// started, and the busiest of each app below.
    pub processes: Vec<ProcessDto>,
    /// Processes grouped by program, the busiest and largest.
    pub apps: Vec<AppDto>,
    /// What Ahab uses the machine for, busiest first.
    pub ahab: Vec<AhabUseDto>,
    /// Samples of the last 10 minutes, every 3 seconds, oldest first.
    pub history: Vec<SampleDto>,
}
