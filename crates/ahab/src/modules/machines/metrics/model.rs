//! Which processes Ahab started and what for, which to show, and how they
//! group: pure functions over a snapshot of the process table.

use std::collections::{BTreeMap, HashMap, HashSet};

/// One process as the sampler saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
    /// The plugin whose daemon part this is (from its path under
    /// `plugins/run/<id>/`).
    pub plugin: Option<String>,
    /// The agent this program is (`claude-code`, `codex`, `pi`).
    pub agent: Option<&'static str>,
    /// An `ahab terminal-host`.
    pub terminal_host: bool,
}

/// What an Ahab process is for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    /// Ahab itself: the server, the daemon, the terminal host.
    Ahab,
    /// An agent session's processes, by agent.
    Agent(&'static str),
    /// A terminal's shell and what runs in it.
    Terminal,
    /// A plugin's daemon part, by plugin.
    Plugin(String),
}

/// How many processes are shown for CPU, and as many for memory.
pub const TOP: usize = 25;
/// How many apps (processes grouped by name) are shown.
pub const TOP_APPS: usize = 40;
/// How many of an app's processes are listed under it.
pub const APP_MEMBERS: usize = 20;

/// Ahab's processes and what each is for: those named `ahab` (or
/// `ahab-…`), and everything below them, by the child of Ahab it descends
/// from (an agent, a terminal's shell, a plugin's part).
pub fn roles(processes: &[Seen]) -> HashMap<u32, (Role, u32)> {
    let by_pid: HashMap<u32, &Seen> = processes.iter().map(|seen| (seen.pid, seen)).collect();
    let is_root = |seen: &Seen| seen.name == "ahab" || seen.name.starts_with("ahab-");
    let mut found = HashMap::new();
    for process in processes {
        // The chain up to the nearest Ahab process, nearest first.
        let mut chain = Vec::new();
        let mut current = Some(process.pid);
        let mut root = None;
        // A process table can hold a cycle after PID reuse; stop anyway.
        for _ in 0..64 {
            let Some(pid) = current else { break };
            let Some(seen) = by_pid.get(&pid) else { break };
            if is_root(seen) {
                root = Some(*seen);
                break;
            }
            chain.push(*seen);
            current = seen.parent;
        }
        let Some(root) = root else { continue };
        // The child of Ahab this process descends from (its "session").
        let Some(top) = chain.last() else {
            found.insert(process.pid, (Role::Ahab, root.pid));
            continue;
        };
        let role = if root.terminal_host {
            Role::Terminal
        } else if let Some(plugin) = &top.plugin {
            Role::Plugin(plugin.clone())
        } else if let Some(agent) = chain.iter().rev().find_map(|seen| seen.agent) {
            Role::Agent(agent)
        } else {
            Role::Ahab
        };
        found.insert(process.pid, (role, top.pid));
    }
    found
}

/// Ahab's use of the machine by what it is for.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub role: Role,
    /// How many of them: agent sessions, terminals (Ahab's own processes
    /// count one each).
    pub count: usize,
    pub processes: usize,
    pub cpu: f32,
    pub memory: u64,
}

/// Sums Ahab's processes by role, busiest first.
pub fn groups(processes: &[Seen], roles: &HashMap<u32, (Role, u32)>) -> Vec<Group> {
    let mut sums: BTreeMap<Role, (HashSet<u32>, usize, f32, u64)> = BTreeMap::new();
    for process in processes {
        let Some((role, top)) = roles.get(&process.pid) else {
            continue;
        };
        let entry = sums.entry(role.clone()).or_default();
        entry.0.insert(*top);
        entry.1 += 1;
        entry.2 += process.cpu;
        entry.3 += process.memory;
    }
    let mut groups: Vec<Group> = sums
        .into_iter()
        .map(|(role, (tops, processes, cpu, memory))| Group {
            role,
            count: tops.len(),
            processes,
            cpu,
            memory,
        })
        .collect();
    groups.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.memory.cmp(&a.memory)));
    groups
}

/// Processes of one program, together.
#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub name: String,
    pub processes: usize,
    pub cpu: f32,
    pub memory: u64,
    /// Its busiest processes, up to [`APP_MEMBERS`].
    pub members: Vec<u32>,
}

/// Every process grouped by program name; the busiest and largest groups.
pub fn apps(processes: &[Seen]) -> Vec<App> {
    let mut by_name: HashMap<&str, Vec<&Seen>> = HashMap::new();
    for process in processes {
        by_name.entry(&process.name).or_default().push(process);
    }
    let mut apps: Vec<App> = by_name
        .into_iter()
        .map(|(name, mut members)| {
            members.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.memory.cmp(&a.memory)));
            App {
                name: name.to_owned(),
                processes: members.len(),
                cpu: members.iter().map(|seen| seen.cpu).sum(),
                memory: members.iter().map(|seen| seen.memory).sum(),
                members: members
                    .iter()
                    .take(APP_MEMBERS)
                    .map(|seen| seen.pid)
                    .collect(),
            }
        })
        .collect();
    let mut by_cpu: Vec<usize> = (0..apps.len()).collect();
    by_cpu.sort_by(|&a, &b| apps[b].cpu.total_cmp(&apps[a].cpu));
    let mut by_memory: Vec<usize> = (0..apps.len()).collect();
    by_memory.sort_by_key(|&index| std::cmp::Reverse(apps[index].memory));
    let chosen: HashSet<usize> = by_cpu
        .iter()
        .take(TOP_APPS / 2)
        .chain(by_memory.iter().take(TOP_APPS / 2))
        .copied()
        .collect();
    let mut kept: Vec<App> = apps
        .drain(..)
        .enumerate()
        .filter(|(index, _)| chosen.contains(index))
        .map(|(_, app)| app)
        .collect();
    kept.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.memory.cmp(&a.memory)));
    kept
}

/// The processes worth showing one by one: the busiest by CPU, the largest
/// by memory, every one in `always`, and every app's listed members;
/// busiest first.
pub fn shown(processes: &[Seen], always: &HashSet<u32>) -> Vec<u32> {
    let mut by_cpu: Vec<&Seen> = processes.iter().collect();
    by_cpu.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    let mut by_memory: Vec<&Seen> = processes.iter().collect();
    by_memory.sort_by_key(|process| std::cmp::Reverse(process.memory));
    let mut chosen: HashSet<u32> = by_cpu.iter().take(TOP).map(|process| process.pid).collect();
    chosen.extend(by_memory.iter().take(TOP).map(|process| process.pid));
    chosen.extend(always.iter().copied());
    by_cpu
        .into_iter()
        .filter(|process| chosen.contains(&process.pid))
        .map(|process| process.pid)
        .collect()
}

/// A program's name to show: its own, unless it renamed its main thread
/// (Node shows as `MainThread`), then its executable's.
pub fn display_name(name: &str, executable: Option<&str>) -> String {
    if name.ends_with("MainThread")
        && let Some(executable) = executable.and_then(|path| path.rsplit('/').next())
        && !executable.is_empty()
    {
        return executable.to_owned();
    }
    name.to_owned()
}

/// For an interpreter, the script it runs (its file name), from its
/// arguments; nothing else of them is kept.
pub fn script(name: &str, arguments: &[String]) -> Option<String> {
    const INTERPRETERS: [&str; 6] = ["node", "python", "python3", "bun", "deno", "ruby"];
    if !INTERPRETERS.contains(&name) {
        return None;
    }
    arguments
        .iter()
        .skip(1)
        .find(|argument| !argument.starts_with('-'))
        .and_then(|argument| argument.rsplit('/').next())
        .map(str::to_owned)
}

/// The plugin a process belongs to, from a path in its arguments under
/// `plugins/run/<id>/`.
pub fn plugin_of(arguments: &[String]) -> Option<String> {
    arguments.iter().find_map(|argument| {
        let (_, rest) = argument.split_once("/plugins/run/")?;
        let id = rest.split('/').next()?;
        (!id.is_empty()).then(|| id.to_owned())
    })
}

/// The agent a program is, from its name or its script.
pub fn agent_of(name: &str, script: Option<&str>) -> Option<&'static str> {
    match (name, script) {
        ("claude", _) => Some("claude-code"),
        ("codex", _) | (_, Some("codex.js")) => Some("codex"),
        ("pi", _) => Some("pi"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(pid: u32, parent: Option<u32>, name: &str, cpu: f32, memory: u64) -> Seen {
        Seen {
            pid,
            parent,
            name: name.to_owned(),
            cpu,
            memory,
            plugin: None,
            agent: agent_of(name, None),
            terminal_host: false,
        }
    }

    #[test]
    fn ahabs_processes_are_told_apart_by_what_they_are_for() {
        let mut host = seen(30, Some(1), "ahab", 0.0, 1);
        host.terminal_host = true;
        let mut plugin = seen(13, Some(10), "node", 1.0, 50);
        plugin.plugin = Some("docker".to_owned());
        let processes = [
            seen(1, None, "systemd", 0.0, 1),
            seen(10, Some(1), "ahab", 1.0, 100),
            seen(11, Some(10), "claude", 5.0, 200),
            seen(12, Some(11), "bash", 0.5, 10),
            plugin,
            seen(14, Some(10), "git", 0.1, 5),
            host,
            seen(31, Some(30), "zsh", 0.0, 4),
            seen(32, Some(31), "vim", 0.2, 8),
            seen(20, Some(1), "firefox", 9.0, 900),
        ];
        let roles = roles(&processes);
        let role = |pid| roles.get(&pid).map(|(role, _)| role.clone());
        assert_eq!(role(10), Some(Role::Ahab));
        assert_eq!(role(11), Some(Role::Agent("claude-code")));
        assert_eq!(role(12), Some(Role::Agent("claude-code")));
        assert_eq!(role(13), Some(Role::Plugin("docker".to_owned())));
        assert_eq!(role(14), Some(Role::Ahab));
        assert_eq!(role(32), Some(Role::Terminal));
        assert_eq!(role(20), None);

        let groups = groups(&processes, &roles);
        let claude = groups
            .iter()
            .find(|group| group.role == Role::Agent("claude-code"))
            .unwrap();
        assert_eq!((claude.count, claude.processes), (1, 2));
        assert_eq!(claude.memory, 210);
        let terminals = groups
            .iter()
            .find(|group| group.role == Role::Terminal)
            .unwrap();
        assert_eq!((terminals.count, terminals.processes), (1, 2));
    }

    #[test]
    fn processes_group_by_program_busiest_first() {
        let processes = [
            seen(1, None, "docker", 1.0, 10),
            seen(2, None, "docker", 2.0, 10),
            seen(3, None, "zsh", 0.1, 5),
        ];
        let apps = apps(&processes);
        assert_eq!(apps[0].name, "docker");
        assert_eq!(apps[0].processes, 2);
        assert_eq!(apps[0].members, [2, 1]);
        assert_eq!(apps[1].name, "zsh");
    }

    #[test]
    fn the_busiest_the_largest_and_ahabs_are_shown_busiest_first() {
        let mut processes: Vec<Seen> = (0..60_u16)
            .map(|pid| seen(u32::from(pid), None, "x", f32::from(pid), 0))
            .collect();
        processes.push(seen(100, None, "big", 0.0, 1 << 30));
        processes.push(seen(101, None, "ahab", 0.0, 0));
        let shown = shown(&processes, &HashSet::from([101]));
        assert_eq!(shown.first(), Some(&59));
        assert!(shown.contains(&100));
        assert!(shown.contains(&101));
    }

    #[test]
    fn names_scripts_plugins_and_agents_are_read_without_arguments_leaking() {
        assert_eq!(display_name("MainThread", Some("/usr/bin/node")), "node");
        assert_eq!(display_name("claude", Some("/x/2.1.325")), "claude");
        let arguments = [
            "node",
            "--enable-source-maps",
            "/home/me/.local/share/ahab/plugins/run/docker/f69b/dist/daemon.js",
            "--token=x",
        ]
        .map(String::from);
        assert_eq!(script("node", &arguments).as_deref(), Some("daemon.js"));
        assert_eq!(script("claude", &arguments), None);
        assert_eq!(plugin_of(&arguments).as_deref(), Some("docker"));
        assert_eq!(agent_of("node", Some("codex.js")), Some("codex"));
        assert_eq!(agent_of("claude", None), Some("claude-code"));
        assert_eq!(agent_of("pi", None), Some("pi"));
        assert_eq!(agent_of("node", Some("daemon.js")), None);
    }
}
