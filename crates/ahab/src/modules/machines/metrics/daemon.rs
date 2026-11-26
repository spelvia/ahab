//! The daemon's side of `machine.metrics`: this computer's CPU, memory,
//! disks and processes. A sampler runs in the background every 3 seconds
//! while the daemon runs and keeps the last 10 minutes, so charts have a
//! history the moment someone looks; a request answers from its latest
//! sample.

use std::collections::{HashSet, VecDeque};
use std::sync::{LazyLock, Mutex, Once, PoisonError};
use std::time::{Duration, Instant};

use sysinfo::{
    Disks, MINIMUM_CPU_UPDATE_INTERVAL, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind,
    Users,
};
use time::OffsetDateTime;

use crate::modules::machines::metrics::api::{
    AhabUseDto, AppDto, CpuDto, DiskDto, MachineMetricsDto, MemoryDto, ProcessDto, SampleDto,
};
use crate::modules::machines::metrics::model::{
    Role, Seen, agent_of, apps, display_name, groups, plugin_of, roles, script, shown,
};

/// How often the machine is sampled.
const EVERY: Duration = Duration::from_secs(3);
/// How many samples are kept: 10 minutes.
const KEEP: usize = 200;

struct Sampler {
    system: System,
    /// The users processes run as, read again now and then.
    users: Option<Users>,
    /// When the processes were last refreshed, for rates.
    last: Instant,
    ticks: u64,
    history: VecDeque<SampleDto>,
    latest: Option<MachineMetricsDto>,
}

static SAMPLER: LazyLock<Mutex<Option<Sampler>>> = LazyLock::new(|| Mutex::new(None));

/// Starts sampling in the background, once per daemon process.
pub fn start() {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        tokio::spawn(async {
            loop {
                let _ = tokio::task::spawn_blocking(tick).await;
                tokio::time::sleep(EVERY).await;
            }
        });
    });
}

/// `machine.metrics`: the latest sample with the history; the first one is
/// taken now when the sampler has none yet.
pub async fn metrics() -> MachineMetricsDto {
    start();
    if let Some(metrics) = latest() {
        return metrics;
    }
    let _ = tokio::task::spawn_blocking(tick).await;
    latest().unwrap_or_else(empty)
}

fn latest() -> Option<MachineMetricsDto> {
    let sampler = SAMPLER.lock().unwrap_or_else(PoisonError::into_inner);
    let sampler = sampler.as_ref()?;
    let mut metrics = sampler.latest.clone()?;
    metrics.history = sampler.history.iter().cloned().collect();
    Some(metrics)
}

/// Takes one sample and keeps it.
fn tick() {
    let mut kept = SAMPLER.lock().unwrap_or_else(PoisonError::into_inner);
    if kept.is_none() {
        let Ok(mut system) = System::new() else {
            return;
        };
        // CPU use is measured between two refreshes.
        system.refresh_cpu_usage();
        refresh_processes(&mut system);
        std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(250)));
        *kept = Some(Sampler {
            system,
            users: Users::new_with_refreshed_list().ok(),
            last: Instant::now(),
            ticks: 0,
            history: VecDeque::with_capacity(KEEP),
            latest: None,
        });
    }
    let Some(sampler) = kept.as_mut() else {
        return;
    };
    sampler.ticks += 1;
    // Users rarely change: read them again every few minutes.
    if sampler.ticks.is_multiple_of(100) {
        sampler.users = Users::new_with_refreshed_list().ok();
    }
    let elapsed = sampler.last.elapsed().as_secs_f64().max(0.5);
    sampler.last = Instant::now();
    let metrics = sample(&mut sampler.system, sampler.users.as_ref(), elapsed);
    let ahab_cpu = metrics.ahab.iter().map(|group| group.cpu_percent).sum();
    let ahab_memory = metrics.ahab.iter().map(|group| group.memory_bytes).sum();
    if sampler.history.len() == KEEP {
        sampler.history.pop_front();
    }
    sampler.history.push_back(SampleDto {
        at: metrics.sampled_at,
        cpu: metrics.cpu.usage_percent,
        memory_used: metrics.memory.used,
        swap_used: metrics.memory.swap_used,
        load: metrics.cpu.load.map_or(0.0, |load| load[0]),
        ahab_cpu,
        ahab_memory,
    });
    sampler.latest = Some(metrics);
}

fn refresh_processes(system: &mut System) {
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_memory()
            .with_cpu()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_cwd(UpdateKind::OnlyIfNotSet)
            .with_user(UpdateKind::OnlyIfNotSet)
            .with_disk_usage()
            .with_tasks(),
    );
}

fn sample(system: &mut System, users: Option<&Users>, elapsed: f64) -> MachineMetricsDto {
    system.refresh_cpu_usage();
    system.refresh_memory();
    refresh_processes(system);

    // A core count fits an f32 exactly.
    let cores = f32::from(u16::try_from(system.cpus().len().max(1)).unwrap_or(u16::MAX));
    let seen: Vec<Seen> = system
        .processes()
        .values()
        // Threads show as processes on Linux; keep the processes.
        .filter(|process| process.thread_kind().is_none())
        .map(|process| {
            let arguments: Vec<String> = process
                .cmd()
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect();
            let name = display_name(
                &process.name().to_string_lossy(),
                process.exe().and_then(|path| path.to_str()),
            );
            let script = script(&name, &arguments);
            Seen {
                pid: process.pid().as_u32(),
                parent: process.parent().map(sysinfo::Pid::as_u32),
                agent: agent_of(&name, script.as_deref()),
                plugin: plugin_of(&arguments),
                terminal_host: name == "ahab"
                    && arguments
                        .get(1)
                        .is_some_and(|argument| argument == "terminal-host"),
                cpu: process.cpu_usage() / cores,
                memory: process.memory(),
                name,
            }
        })
        .collect();
    let roles = roles(&seen);
    let ahab: HashSet<u32> = roles.keys().copied().collect();
    let per_second = |bytes: u64| -> u64 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a rate in whole bytes per second"
        )]
        let rate = (bytes as f64 / elapsed) as u64;
        rate
    };
    let apps = apps(&seen);
    let mut wanted: HashSet<u32> = ahab.clone();
    wanted.extend(apps.iter().flat_map(|app| app.members.iter().copied()));
    let processes = shown(&seen, &wanted)
        .into_iter()
        .filter_map(|pid| {
            let process = system.process(sysinfo::Pid::from_u32(pid))?;
            let found = seen.iter().find(|seen| seen.pid == pid)?;
            let arguments: Vec<String> = process
                .cmd()
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect();
            Some(ProcessDto {
                pid,
                script: script(&found.name, &arguments),
                name: found.name.clone(),
                cpu_percent: found.cpu,
                memory_bytes: found.memory,
                started_at: i64::try_from(process.start_time())
                    .ok()
                    .and_then(|at| OffsetDateTime::from_unix_timestamp(at).ok()),
                ahab: ahab.contains(&pid),
                role: roles.get(&pid).map(|(role, _)| match role {
                    Role::Ahab => "ahab".to_owned(),
                    Role::Agent(agent) => format!("agent:{agent}"),
                    Role::Terminal => "terminal".to_owned(),
                    Role::Plugin(plugin) => format!("plugin:{plugin}"),
                }),
                parent: found.parent,
                user: process
                    .user_id()
                    .and_then(|uid| users?.get_user_by_id(uid))
                    .map(|user| user.name().to_owned()),
                status: process.status().to_string(),
                threads: process.tasks().map(HashSet::len),
                cpu_time_ms: process.accumulated_cpu_time(),
                run_seconds: process.run_time(),
                virtual_bytes: process.virtual_memory(),
                read_per_second: per_second(process.disk_usage().read_bytes),
                written_per_second: per_second(process.disk_usage().written_bytes),
                executable: process.exe().map(|path| path.display().to_string()),
                cwd: process.cwd().map(|path| path.display().to_string()),
            })
        })
        .collect();

    MachineMetricsDto {
        sampled_at: OffsetDateTime::now_utc(),
        host_name: System::host_name().ok(),
        os_version: System::long_os_version().ok(),
        kernel: System::kernel_version().ok(),
        uptime_seconds: System::uptime().unwrap_or_default(),
        cpu: CpuDto {
            usage_percent: system.global_cpu_usage(),
            cores: system.cpus().iter().map(sysinfo::Cpu::usage).collect(),
            physical_cores: System::physical_core_count().ok(),
            load: System::load_average()
                .ok()
                .map(|load| [load.one, load.five, load.fifteen]),
        },
        memory: MemoryDto {
            total: system.total_memory(),
            used: system.used_memory(),
            available: system.available_memory(),
            swap_total: system.total_swap(),
            swap_used: system.used_swap(),
        },
        disks: disks(),
        process_count: seen.len(),
        processes,
        apps: apps
            .into_iter()
            .map(|app| AppDto {
                name: app.name,
                processes: app.processes,
                cpu_percent: app.cpu,
                memory_bytes: app.memory,
                members: app.members,
            })
            .collect(),
        ahab: groups(&seen, &roles)
            .into_iter()
            .map(|group| {
                let (kind, id) = match group.role {
                    Role::Ahab => ("ahab", None),
                    Role::Agent(agent) => ("agent", Some(agent.to_owned())),
                    Role::Terminal => ("terminal", None),
                    Role::Plugin(plugin) => ("plugin", Some(plugin)),
                };
                AhabUseDto {
                    kind: kind.to_owned(),
                    id,
                    count: group.count,
                    processes: group.processes,
                    cpu_percent: group.cpu,
                    memory_bytes: group.memory,
                }
            })
            .collect(),
        history: Vec::new(),
    }
}

/// The real disks, once each: pseudo file systems and repeated mounts of one
/// device are left out.
fn disks() -> Vec<DiskDto> {
    const PSEUDO: [&str; 9] = [
        "tmpfs", "devtmpfs", "overlay", "squashfs", "proc", "sysfs", "ramfs", "efivarfs", "nsfs",
    ];
    let Ok(disks) = Disks::new_with_refreshed_list() else {
        return Vec::new();
    };
    let mut devices = HashSet::new();
    disks
        .iter()
        .filter(|disk| {
            let file_system = disk.file_system().to_string_lossy();
            disk.total_space() > 0 && !PSEUDO.contains(&file_system.as_ref())
        })
        .filter(|disk| devices.insert(disk.name().to_string_lossy().into_owned()))
        .map(|disk| DiskDto {
            mount: disk.mount_point().display().to_string(),
            file_system: disk.file_system().to_string_lossy().into_owned(),
            total: disk.total_space(),
            available: disk.available_space(),
        })
        .collect()
}

fn empty() -> MachineMetricsDto {
    MachineMetricsDto {
        sampled_at: OffsetDateTime::now_utc(),
        host_name: None,
        os_version: None,
        kernel: None,
        uptime_seconds: 0,
        cpu: CpuDto {
            usage_percent: 0.0,
            cores: Vec::new(),
            physical_cores: None,
            load: None,
        },
        memory: MemoryDto {
            total: 0,
            used: 0,
            available: 0,
            swap_total: 0,
            swap_used: 0,
        },
        disks: Vec::new(),
        process_count: 0,
        processes: Vec::new(),
        apps: Vec::new(),
        ahab: Vec::new(),
        history: Vec::new(),
    }
}
