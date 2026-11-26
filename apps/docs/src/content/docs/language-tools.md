---
title: LSPs and Problems
description: Language servers, linters, formatters and check commands on your machines, Problems, installing tools, the project's environment, and your own tools.
---

Language servers, linters, formatters and check commands run **on the
project's machine**, where the code and its toolchains are. You open a
file and it works: highlighted at once, then errors, hover, go to
definition and references a few seconds later, from whichever tool the
project, the machine or Ahab provides. Ahab calls these tools **LSPs**.

:::note[Status]
Everything on this page works today, except project symbols (`#` in the
palette) and inlay hints, which are not built yet, and call and type
hierarchies, which come later.
:::

## What runs

Each language lists its tools in order of preference. The first that can
run serves the language, unless the project says otherwise.

| Language                    | Tools                                                                           |
| --------------------------- | ------------------------------------------------------------------------------- |
| Rust                        | rust-analyzer, rustfmt, cargo clippy                                            |
| TypeScript, TSX, JavaScript | typescript-language-server, biome, ESLint, prettier; tsc (not for JavaScript)   |
| JSON, CSS, SCSS, HTML       | the VS Code JSON, CSS and HTML servers; biome (JSON, CSS) and prettier          |
| Python                      | basedpyright, ruff                                                              |
| Go                          | gopls, gofmt                                                                    |
| Java                        | jdtls, google-java-format                                                       |
| Nix                         | nil, nixfmt                                                                     |
| TOML, YAML, Markdown        | taplo; yaml-language-server and prettier; marksman and prettier                 |
| Bash                        | bash-language-server, ShellCheck, shfmt                                         |
| Lua, C, C++, Dockerfile     | lua-language-server and StyLua; clangd; dockerfile-language-server and hadolint |

There are four kinds of tool:

- **Language servers** (LSP) give problems, hover, definitions, references
  and, while editing, completion, rename, fixes and formatting.
- **Linters** run on a file when it is saved: ShellCheck, hadolint.
- **Formatters** read a file and write it back formatted: rustfmt,
  prettier, gofmt.
- **Check commands** look at the whole project, which language servers
  often do not: `cargo clippy`, `tsc --noEmit`.

Biome, ESLint, prettier and StyLua run only where the project configures
them (a `biome.json`, an `eslint.config.js`, a `.prettierrc`…), so a
TypeScript project gets whichever of biome and ESLint it uses.

## Where it shows

### In the file window

The [status bar](/docs/editor/#the-status-bar) shows the focused file's
language and, beside `{}`, its tools' state:

| Shows                 | Means                                                     |
| --------------------- | --------------------------------------------------------- |
| `Rust` `{} starting…` | the server is starting (rust-analyzer can take a minute)  |
| `Rust` `{} ✓`         | every chosen tool runs                                    |
| `Rust` `{} 2 issues`  | a tool failed or is missing; the rest runs                |
| `Java` `{} basic`     | no server: highlighting and approximate navigation only   |
| `Markdown` `{}`       | nothing is configured for this language; nothing is wrong |

Click `{}` for the tools behind it: each with its version and where it was
found, the root it runs in, the environment, and **Restart** and **Logs**.
When a tool is missing, it says why ("jdtls not on atlas · needs Java 21 or
later (found: Java 17)") and offers to install it, set its path, or turn
the language's tools off for this project. Nothing pops up by itself, and
nothing blocks reading.

### Navigation and editing help

| What                            | Key                                | Needs Edit |
| ------------------------------- | ---------------------------------- | ---------- |
| Go to definition                | F12, ⌘-click (Ctrl-click)          | no         |
| Type definition, implementation | the context menu                   | no         |
| Find references                 | ⇧F12                               | no         |
| Hover                           | hover; a long press on touch       | no         |
| Project symbols                 | `#` in the palette (not built yet) | no         |
| Inlay hints                     | not built yet; off by default      | no         |
| Completion, signature help      | while typing                       | yes        |
| Rename                          | F2, with a preview                 | yes        |
| Code actions, quick fixes       | ⌘.                                 | yes        |
| Format                          | ⇧⌥F, or format on save             | yes        |

Navigation works in read-only files; only what changes files needs the
**Edit** switch (see [Editor](/docs/editor/#editing)). Edits from tools go
through the same versioned saves as yours.

A file has one formatter: the one the project chose, else a tool the
project configures (biome, prettier), else a language server that formats,
else a formatter command.

### Definitions outside the project

Definitions often land outside the project: the standard library, a
crate in `~/.cargo/registry`, `node_modules`, the JDK. They open as
**external files**: read-only, marked with where they come from ("rust
std", "node_modules/react"). The machine serves such a file only because a
language server returned it, and only while that server runs; files over
2 MB are not served. Nothing outside the project can be browsed.

### Without a language server

While no server runs (not installed, starting, crashed), go to definition
and find references work **approximately**, from the syntax tree's names
and a search of the project. Results say "approximate" and are replaced
by the server's once it is ready.

Problems show in the text itself too: see
[Problems in the editor](/docs/editor/#problems-in-the-editor).

### Problems

**Problems** is a tab of the [bottom panel](/docs/editor/#the-bottom-panel)
(⇧⌘M, or click the counts `⊗ 2 ⚠ 1` in the status bar):

- grouped by file, and first by repository and worktree when the
  project holds several ([Many repositories](#many-repositories));
  **collapse all** folds them;
- the **filter** takes words, found in the message, code, tool or path,
  and globs over paths, separated by commas: `unused, **/*.ts,
  !**/test/**`; `!` leaves out what it matches. The funnel chooses the
  severities, one tool and one repository, and turns a plugin's problems
  off for the project;
- a click opens the file at the problem;
- **▶ Check project** runs the check commands now (what they said also
  goes to Output's **Check runs**); with more than 5 roots it first asks
  which repositories to check.

Plugins add problems too: as tool records
([`contributes.tools`](/docs/plugins/manifest/#contributestools)), or from a
daemon entry through the [diagnostics API](/docs/plugins/daemon-entry/#problems);
a plugin's show as `<plugin>.<collection>` and can be turned off per
project like a tool.

With nothing to show it says "No problems have been detected in the
project." Problems was a window of its own before; a layout saved with
it opens the panel instead.

### Output

**Output** is the bottom panel's other tab (⇧⌘U): pick a source and read
it, following new lines while scrolled to the end, with **Clear** (here
only) and **Copy**:

- each language server running on the project's machine (its log:
  what it printed, and its LSP traffic when the machine keeps it);
- **Check runs**: Check project's results from this browser;
- each plugin with a daemon entry: its logs on that machine.

The server keeps the latest problems of each project, so another client
sees them while the machine is busy, and a turn's before and after can be
compared.

### After an agent's turn

When an agent's turn ends, the check commands that run after turns run at
the roots of the files the turn changed (none when it changed none), and
the chat shows what changed: "3 new errors, 1 fixed", expandable.
**Send to agent** puts them in the composer; nothing is sent by itself.

### Settings → Machines → _machine_ → LSPs

What one machine has:

- its **runtimes**: Node, Java, Python, Go and Rust, with their versions,
  and **Scan again**;
- every tool with its state there: **installed by Ahab**, **found** (and
  where: the project, `PATH` with fnm, rustup or Nix…), **available** to
  install, **cannot run here** (a missing runtime, no build for this
  platform), or **off**; Ahab's defaults are marked;
- **Install**, **Update** and **Remove**, with progress and, when one
  fails, the step and its output;
- the running servers with their root, memory, **Restart** and **Logs**,
  and their memory together against the machine's
  [memory budget](#the-memory-budget); the LSP traffic is kept in the logs
  when you turn that on;
- the machine's settings: a tool's path, extra environment (`JAVA_HOME`),
  tools turned off, how many servers and checks may run at once, the
  memory budget, and your own tools.

### Project settings → LSPs

Choices that follow the project, wherever its folder is:

- per language, which tools in which order ("biome, not ESLint"), which
  one formats, format on save, or no tools at all;
- the environment: what was found (direnv, mise), and the switch for a Nix
  dev shell;
- check commands: which run, and when (on save, after agent turns, on
  request);
- when language servers start: **when I open a file** (the default),
  **only in pinned repositories**, or **off**; and how many days a
  repository stays active (14);
- plugins' problems turned off (from Problems), to turn on again;
- the repository's `.ahab/tools.json`, when it has one, shown as the
  source of those choices.

## Many repositories

A project can hold many repositories: a folder of clones, a bare
repository with dozens of worktrees, a repository with submodules. Ahab
finds them as the Git view does, and tools work within them.

| Found                                                                                             | Is                                                     |
| ------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| a `.git` folder                                                                                   | a **repository**                                       |
| a `.git` file naming a repository's worktree, or a bare repository (the `gitdir: ./.bare` layout) | a **worktree**, under its repository                   |
| a `.git` file pointing into a repository's `.git/modules/`, or a folder its `.gitmodules` lists   | a **submodule**: part of its repository, never its own |
| no `.git` above a file, up to the project's folder                                                | the project stands for the repository                  |

Repositories are looked for up to 5 folders deep (worktrees of the bare
layout as deep as their repository), at most 300, skipping
`node_modules`, `target`, `vendor`, `dist`, `build` and `.venv`. The
daemon keeps what it found and looks again only when a `.git` entry, a
Git folder or a folder it looked into changes. A tool's root is found
within its repository or worktree, never across two, and a root never
holds another repository.

**Nothing runs until you touch it.**

- A language server starts only for the root of a file you open: one Java
  file in one worktree starts one jdtls, for that worktree.
- Checks never run across the whole project by themselves: on save, at
  the saved file's root; after an agent's turn, at the roots of the files
  it changed; **Check project**, everywhere, but with more than 5 roots
  it asks which repositories and worktrees, the active and pinned ones
  chosen. At most 2 checks run at once on a machine (a machine setting,
  1 to 16); the rest wait their turn.
- Each machine keeps the roots it found in each repository and worktree,
  and looks again only in those where a folder changed (a root marker or
  a `.gitignore` added or removed), so the chooser opens at once after the
  first time.
- A project can start servers **only in pinned repositories**, or turn
  them **off**; its files then say "not started" and why.

### Active and pinned repositories

A repository, or each of its worktrees, is **active** when a file in it
was opened, or an agent's turn changed one, in the last 14 days (a
project setting). **Pin** keeps one active always; **Unpin** returns it
to the rule. Pin and Unpin are on each repository and worktree in
Problems and in the Git view; pinning a repository pins all its
worktrees.

Problems shows active and pinned repositories, and those with problems;
the rest are one line, "12 repositories not checked", which opens Check
project's chooser. It groups **repository → worktree → file**; a
project with one repository shows files directly. The server keeps
when each was used and the pins, per project.

### The memory budget

Each machine has a memory budget for its language servers: half its
memory unless set (Settings → Machines → _machine_ → LSPs, in MiB).
Ahab checks the budget periodically and before starting another server,
estimating its startup cost from previous runs (256 MiB for a new tool).
When it needs room, it stops unused servers starting with the one idle
longest. Servers with open documents can exceed the budget. The LSPs
tab includes the memory of each server and its child processes.

## How tools run

- A tool starts the first time a file of its language opens under its
  **root**: the nearest folder with one of its root markers (`Cargo.toml`,
  `package.json`, `pom.xml`), looking up from the file and stopping at its
  repository's or worktree's folder (or the project's). rust-analyzer and cargo clippy take the top-most
  `Cargo.toml` with `[workspace]`. Without a marker, the root is the
  project's folder.
- **One server per root and tool** runs on a machine, shared by every
  window, client and project under that root, and by every language it
  serves (one typescript-language-server for TypeScript and JavaScript).
- A server stays warm for **3 minutes** after its last document closes.
  When the machine reaches the configured server count (8 by default,
  configurable from 1 to 64), Ahab makes room from unused servers first.
  Open documents can keep more servers running.
- A server that crashes restarts after 1, 2, then 4 seconds. When it keeps
  crashing (more than three times in five minutes), Ahab stops restarting
  it and says so; **Restart** tries again.
- A linter runs on a file when it is saved. A check command runs on save
  (at the saved file's root), after an agent's turn (at the roots of the
  files it changed) and on request (at every root, or the repositories
  chosen), unless the record or the project says otherwise (tsc does not
  run on save). A check may take up to 15 minutes; a check's problems
  replace only its root's.
- Machines on an Ahab from before language tools show "update Ahab on this
  machine for language tools"; the rest of Ahab works with them as before.

### Finding a tool

The first place that has the tool wins, unless a setting names its path:

1. **The project:** `node_modules/.bin`, `.venv/bin` or `venv/bin` at or
   above the root, and for cargo, rustfmt, rust-analyzer and clippy the
   toolchain a `rust-toolchain.toml` asks for (`rustup which`).
2. **The project's environment:** the tool on `PATH` inside direnv, mise,
   or the Nix dev shell when it is turned on.
3. **Installed by Ahab** on this machine.
4. **The machine:** your login shell's `PATH`, as agents are found.

### The environment

A tool starts with your login shell's environment, then the project's
own, the first that applies:

- **direnv:** an `.envrc` at or above the root that you allowed runs the
  tool under `direnv exec`. An `.envrc` you have not allowed is shown
  ("run `direnv allow` in a terminal to use it") and the tool runs
  without it: Ahab never allows one, as that runs the project's code.
- **mise or asdf:** a `.mise.toml`, `mise.toml` or `.tool-versions` runs
  the tool under `mise exec`.
- **A Nix dev shell:** a `flake.nix` in the project, only when the
  project turns it on, as `nix develop` can take long. Its environment is
  kept per `flake.lock` in the machine's data folder, so it is built once.
- **Extra variables** from the machine's and the project's settings.

The environment is loaded again when its file changes. Turn direnv or mise
off for a project in its settings.

### Runtimes

Many tools need a runtime: the npm ones need Node, jdtls Java 21 or later,
gopls Go. Ahab looks for Node, Java, Python, Go and Rust on each machine
and shows them in its LSPs tab. A tool whose runtime is missing says so
("needs Java 21 or later (found: Java 17)") instead of failing. Node is
also the one Ahab uses for plugins when `PATH` has none. Ahab never
installs a runtime.

Java has two JDKs: the one that runs jdtls (any recent one on the
machine) and the one the project compiles against, from its build files
or its environment, which is why tools start in the project's environment.

## Installing tools

Ahab keeps its own registry of tools: where each comes from and how to run
it. A machine's LSPs tab, or `ahab lsp install`, installs a tool on that
machine, as mason.nvim does:

- into `<data folder>/tools/<id>/<version>/` on that machine, with a
  `current` link to the version in use; never system-wide, never with
  `sudo`;
- at the version the registry pins; a downloaded file is checked against
  the registry's SHA-256 and refused when it differs;
- with npm, pip in a virtual environment of its own, `cargo install
  --locked`, `go install`, or a release download, so installing an npm
  tool needs Node, a Python one Python 3, and so on; a missing runtime
  stops the install before it starts.

**Update** installs the registry's newer version, moves `current`, deletes
the older version and restarts the tool's servers. **Remove** stops its
servers and deletes its folder. Some tools come from your toolchain and are
never installed by Ahab: rustfmt and cargo clippy (rustup), gofmt (Go);
Ahab finds them and says how to get them.

The registry is bundled with Ahab; a newer Ahab brings newer versions.
See [Tool registry](/docs/development/tool-registry/) for its format.

## Settings

How a tool _behaves_ is the project's own files: `tsconfig.json`,
`pyproject.toml`, `rust-analyzer.toml`, `.eslintrc`. Ahab's settings say
only which tool, where, and how it starts. They come in layers; a later
layer wins, field by field:

1. Ahab's defaults: the registry and each language's order.
2. Plugins, which add tools or replace a default (see
   [`contributes.tools`](/docs/plugins/manifest/#contributestools)).
3. The machine.
4. The repository's `.ahab/tools.json`.
5. The project in Ahab.

### `.ahab/tools.json`

A repository can share its choices with everyone who opens it. Ahab reads
the nearest `.ahab/tools.json` from a file's folder up to the project's
folder; the project's settings in Ahab still win over it. A file Ahab
cannot read is ignored.

```json
{
	"languages": {
		"typescript": {
			"tools": ["typescript-language-server", "biome", "tsc"],
			"formatOnSave": true
		},
		"python": { "formatter": "ruff" }
	},
	"tools": {
		"cargo-clippy": { "on": ["agentTurn", "request"] },
		"basedpyright": {
			"settings": { "basedpyright": { "analysis": { "typeCheckingMode": "standard" } } }
		}
	},
	"environment": { "nix": true }
}
```

| Field                         | What                                                                                    |
| ----------------------------- | --------------------------------------------------------------------------------------- |
| `languages.<id>.tools`        | The language's tools, in order, instead of the defaults                                 |
| `languages.<id>.off`          | No tools for this language                                                              |
| `languages.<id>.formatOnSave` | Format on save                                                                          |
| `languages.<id>.formatter`    | The tool that formats                                                                   |
| `tools.<id>.off`              | Do not run this tool                                                                    |
| `tools.<id>.path`             | Where the tool is, instead of looking for it                                            |
| `tools.<id>.env`              | Extra environment                                                                       |
| `tools.<id>.settings`         | A language server's settings, over the record's; `init` goes to `initializationOptions` |
| `tools.<id>.on`               | When a check runs: `save`, `agentTurn`, `request`                                       |
| `environment.nix`             | Use the flake's dev shell (off by default)                                              |
| `environment.direnv`, `.mise` | Use direnv, mise (on by default)                                                        |
| `environment.env`             | Extra variables for every tool                                                          |
| `records`                     | Your own tools (below)                                                                  |

A machine's settings take the same fields, plus `maxServers` (how many
language servers run at once, default 8) and `logTraffic` (keep the LSP
traffic in the servers' logs).

### Your own tools

The machine and the project can declare tools of their own under
`records`, in the [registry's format](/docs/development/tool-registry/)
without a `source`: Ahab finds them, and never installs them. They serve
their languages after the defaults. For example, a linter that prints
`file:line:col: severity: message`:

```json
{
	"records": [
		{
			"id": "my-linter",
			"name": "my-linter",
			"kind": "linter",
			"languages": ["python"],
			"run": {
				"command": ["my-linter", "${path}"],
				"rootMarkers": ["pyproject.toml"],
				"output": {
					"format": "pattern",
					"pattern": {
						"regexp": "^(?P<file>[^:]+):(?P<line>\\d+):(?P<column>\\d+): (?P<severity>\\w+): (?P<message>.*)$"
					}
				}
			}
		}
	]
}
```

A check (or linter) can read **report files** instead of its output,
after it ends: `"reports"` beside its `format`, globs relative to the
root. Only reports written while it ran count, and ignore files do not
hide them (`build/` usually is ignored). Gradle and Maven tasks write
their reports there; with **Checkstyle XML** (`"format": "checkstyle"`),
which Checkstyle, PMD, SpotBugs, ktlint and detekt write, a build's
checks become problems without reading its console:

```json
{
	"id": "gradle-check",
	"name": "Gradle check",
	"kind": "check",
	"languages": ["java", "kotlin"],
	"run": {
		"command": ["./gradlew", "check", "--console=plain"],
		"rootMarkers": ["settings.gradle.kts", "settings.gradle"],
		"on": ["request"],
		"output": {
			"format": "checkstyle",
			"reports": ["build/reports/checkstyle/*.xml", "*/build/reports/checkstyle/*.xml"]
		}
	}
}
```

The output formats, in Ahab's order of preference: LSP (a server),
SARIF, Checkstyle XML, a declared JSON shape, a line pattern.

A repository's `.ahab/tools.json` can only choose among the tools Ahab,
plugins, the machine and the project already know; `records` in it are
ignored, and the server's log says so. A tool is a command, and a cloned
repository (or an agent writing that file) must not make Ahab run one just
because a file was opened. Declare such a tool in the project's or the
machine's settings instead.

Language servers themselves can still run a project's code: rust-analyzer
runs build scripts and procedural macros, and eslint loads its JavaScript
configuration. Add only folders you trust as projects, as you would
before opening them in any editor.

## The command line

```sh
ahab lsp list [--machine <machine>] [--language <id>] [--rescan]
ahab lsp install <tool> [--machine <machine>] [--no-wait]
ahab lsp update <tool> [--machine <machine>] [--no-wait]
ahab lsp remove <tool> [--machine <machine>] [--no-wait]
ahab lsp status [--machine <machine>]
ahab problems [--project <id>] [--severity <severity>] [--source <tool>] [--path <path>]
ahab check [--project <id>] [--tool <tool>]… [--repository <folder>]…
```

See [CLI](/docs/installation/cli/#language-tools) for what each does.

## API

| Route                                                              | What                                                              |
| ------------------------------------------------------------------ | ----------------------------------------------------------------- |
| `GET /api/v1/machines/{id}/tools?rescan=`                          | Every tool with its state on the machine, its runtimes, and jobs  |
| `POST /api/v1/machines/{id}/tools/{tool}/install`                  | Start installing; `/update` and `/remove` likewise (`202`, a job) |
| `GET /api/v1/machines/{id}/language-servers`                       | The running servers: root, state, memory, open files              |
| `POST /api/v1/machines/{id}/language-servers/{server}/restart`     | Restart one                                                       |
| `GET /api/v1/machines/{id}/language-servers/{server}/log`          | Its log                                                           |
| `GET`/`PUT /api/v1/machines/{id}/tool-settings`                    | The machine's settings                                            |
| `GET`/`PUT /api/v1/projects/{id}/tool-settings`                    | The project's settings                                            |
| `GET /api/v1/projects/{id}/problems?severity=&source=&path=&text=` | A project's problems, with counts                                 |
| `POST /api/v1/projects/{id}/checks`                                | Run the check commands now (`{tools}`, `{checkouts}` for some)    |
| `GET /api/v1/projects/{id}/check-roots`                            | Where Check project would run, by repository; when it asks        |
| `GET /api/v1/projects/{id}/repository-activity`                    | The repositories used or pinned, and which are active             |
| `PUT /api/v1/projects/{id}/repository-pins`                        | Pin or unpin a repository or worktree                             |
| `GET /api/v1/projects/{id}/problems/summary`                       | Counts per project                                                |
| `GET /api/v1/chats/{id}/turns/{turn}/diagnostics`                  | What a turn changed                                               |

Requests from file windows to language servers go over a channel to the
machine, not over these routes.
