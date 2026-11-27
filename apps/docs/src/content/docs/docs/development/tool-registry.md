---
title: Tool registry
description: The format of Ahab's tool records, their sources and output adapters, and how to add or update a tool.
---

Ahab keeps its own registry of language tools: one record per tool, in
`tools/registry/<id>.json`, saying both **how to get it** and **how to
run it**. The tools themselves come from their own sources; the registry
holds only recipes, with pinned versions and checksums. Each language in
`apps/ui/languages/languages.json` lists the tools that serve it under
`tools`, by record ID, in order of preference. What people see is in
[LSPs and Problems](/docs/language-tools/).

The same format serves plugins' [`contributes.tools`](/docs/plugins/manifest/#contributestools)
and your own tools in settings and `.ahab/tools.json`.

:::note[Status]
The registry is bundled in Ahab: a new tool version reaches people with an
Ahab release. A separate registry repository, so tools update without a
release, is planned; the record format stays the same.
:::

## A record

```json
{
	"id": "rust-analyzer",
	"name": "rust-analyzer",
	"kind": "languageServer",
	"languages": ["rust"],
	"license": "MIT OR Apache-2.0",
	"homepage": "https://rust-analyzer.github.io",
	"description": "The Rust language server.",
	"source": {
		"type": "githubRelease",
		"repo": "rust-lang/rust-analyzer",
		"version": "2026-11-16",
		"assets": {
			"linux-x64": {
				"file": "rust-analyzer-x86_64-unknown-linux-gnu.gz",
				"sha256": "4a08727a…"
			},
			"darwin-arm64": {
				"file": "rust-analyzer-aarch64-apple-darwin.gz",
				"sha256": "608b7c7d…"
			}
		}
	},
	"bin": { "rust-analyzer": "rust-analyzer" },
	"run": {
		"command": ["rust-analyzer"],
		"rootMarkers": ["Cargo.toml", "rust-project.json"],
		"topMostContaining": "[workspace]",
		"settingsFiles": ["rust-analyzer.toml"]
	},
	"hint": "rustup component add rust-analyzer"
}
```

| Field         | Type   | Required        | Meaning                                                                                                    |
| ------------- | ------ | --------------- | ---------------------------------------------------------------------------------------------------------- |
| `id`          | string | yes             | Lowercase letters, digits, dots and dashes, at most 64. Stable: never renamed. The file is `<id>.json`.    |
| `name`        | string | yes             | The name people see.                                                                                       |
| `kind`        | string | yes             | `languageServer`, `formatter`, `linter` or `check`.                                                        |
| `languages`   | array  | no              | Ahab's language IDs it serves (`rust`, `tsx`).                                                             |
| `license`     | string | in the registry | The tool's licence, as an SPDX expression.                                                                 |
| `homepage`    | string | no              | Where to read about it.                                                                                    |
| `description` | string | no              | One line, shown in the LSPs tab.                                                                           |
| `source`      | object | in the registry | Where it comes from: see [Sources](#sources). Absent for your own tools, which are found, never installed. |
| `requires`    | array  | no              | Runtimes it needs to run: `[{ "runtime": "java", "min": 21 }]`. See [Runtimes](#runtimes).                 |
| `bin`         | object | no              | Command names to their path in the installed tool's folder. Default: the command's first word.             |
| `run`         | object | yes             | How it starts: see [`run`](#run).                                                                          |
| `hint`        | string | no              | How to get it without Ahab, shown when it is missing (`brew install jdtls`).                               |
| `replaces`    | string | no              | For plugins: the ID of a tool this one takes the place of.                                                 |

Readers ignore fields they do not know. A `kind` or source `type` from a
newer Ahab is listed but never run or installed.

### `run`

| Field                   | Meaning                                                                                                                               |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `command`               | The command, as a list. Its first word is one of `bin`'s names (or a path). Required.                                                 |
| `rootMarkers`           | Files or folders that mark a root, looked for from the file up to the project's folder; the nearest wins. None: the project's folder. |
| `topMostContaining`     | Prefer the top-most marker whose text contains this (`[workspace]` in `Cargo.toml`).                                                  |
| `settingsFiles`         | The project's own files that configure the tool, shown in its status.                                                                 |
| `configFiles`           | The tool runs only where one of these exists between the file and the project's folder (biome, ESLint, prettier).                     |
| `initializationOptions` | A language server's `initializationOptions`.                                                                                          |
| `settings`              | What a language server gets for `workspace/configuration`, by section; settings in layers go over it.                                 |
| `output`                | A linter's or a check's output: see [Output adapters](#output-adapters). Required for those kinds.                                    |
| `versionArgs`           | How to ask its version. Default `["--version"]`; `[]` when it has no such flag.                                                       |
| `env`                   | Extra environment.                                                                                                                    |
| `on`                    | When a check runs: `save`, `agentTurn`, `request`. Default: all three.                                                                |
| `languageIds`           | The LSP language IDs the server knows Ahab's languages by, when they differ (`{"tsx": "typescriptreact"}`; common ones are built in). |

In `command`, `${path}` is the file (formatters and linters), `${root}`
the root, `${toolDir}` the installed tool's folder, and `${cache}` a
folder for the tool's own data per root (jdtls's `-data`). In `settings`
and `initializationOptions` strings, `${root}`, `${rootUri}` and
`${rootName}` are the root's path, address and name.

What each kind's command does:

- **`languageServer`:** speaks LSP on stdin and stdout.
- **`formatter`:** reads the file on stdin and prints it formatted
  (`prettier --stdin-filepath ${path}`, `ruff format -`).
- **`linter`:** runs on one file (`${path}`) when it is saved and prints
  problems in its declared `output`.
- **`check`:** runs at each root in the project and prints the whole
  project's problems in its declared `output`, for up to 15 minutes.

### Sources

| `type`          | Fields                                                 | Installs with                                                    | Needs  |
| --------------- | ------------------------------------------------------ | ---------------------------------------------------------------- | ------ |
| `npm`           | `package`, `version`, `extra` (pinned `name@version`s) | `npm install --prefix <tool folder>` the package and `extra`     | Node   |
| `pypi`          | `package`, `version`                                   | a virtual environment of its own, `pip install package==version` | Python |
| `cargo`         | `crate`, `version`, `git`, `features`                  | `cargo install --locked --root <tool folder>`                    | Rust   |
| `go`            | `module`, `version`                                    | `go install module@version` with `GOBIN` the tool folder         | Go     |
| `githubRelease` | `repo`, `version`, `tag`, `assets`                     | download the platform's file, check its SHA-256, unpack          |        |
| `download`      | `version`, `assets` (with `url`)                       | the same, from another address (jdtls, from Eclipse)             |        |
| `system`        |                                                        | never: found, and explained by `hint` (rustfmt, gofmt, clippy)   |        |

- **`cargo` with `git`** installs from that Git repository at the tag
  `version` (nil, which is not on crates.io).
- **`tag`** is the release's tag when it is not the version itself,
  written with `${version}`: `"v${version}"`.
- **`assets`** maps a platform to its file: `linux-x64`, `linux-arm64`,
  `darwin-x64`, `darwin-arm64`, or `any` for one file for every platform.
  Each has `file` (`githubRelease`) or `url` (`download`), both may use
  `${version}`; `sha256`, the file's SHA-256 in hex; and `bin` when the
  binaries' paths differ per platform (an archive with a folder inside).
  A platform without an asset with a `sha256` cannot install the tool.
- **Files** may be `.tar.gz`, `.tgz`, `.tar` or `.zip` archives, or one
  binary, plain or gzipped, which becomes the first `bin`.

A tool installs into `<data folder>/tools/<id>/<version>/` on the machine,
with a `current` link to the version in use and an `.ahab-tool.json`
describing it.

### Runtimes

`requires` names what the tool needs to **run**: `node`, `java`,
`python`, `go` or `rust`, with the lowest major version in `min`. What
installing needs (Node for `npm`, Python for `pypi`…) follows from the
source and need not be listed. A daemon finds runtimes on the machine's
`PATH` and reports a tool whose runtime is missing or too old as unable to
run, with what it needs.

### Output adapters

Language servers give LSP's Diagnostic as it is. Linters and checks
declare how their output becomes one, in `run.output`; prefer, in order,
SARIF, Checkstyle XML, a JSON shape, a line pattern.

| Field     | Meaning                                                                                                                                |
| --------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `format`  | `sarif`, `checkstyle`, `json` or `pattern`.                                                                                            |
| `stream`  | What to read: `stdout` (default), `stderr` or `both`.                                                                                  |
| `json`    | The JSON shape, for `json`.                                                                                                            |
| `pattern` | The line pattern, for `pattern`.                                                                                                       |
| `reports` | Report files to read after the command ends, instead of its output: globs relative to the root; only those written while it ran count. |

Paths in the output may be absolute, `file://` addresses, or relative to
the root. Problems outside the project are dropped. A tool that exits
with an error and prints nothing readable is reported as failed, with its
last line.

**`sarif`:** SARIF 2.1, each run's results at their first physical
location (`eslint -f sarif`, hadolint).

**`checkstyle`:** Checkstyle's XML (`<checkstyle><file name><error line
column severity message source/>`), which Checkstyle, PMD, SpotBugs, ktlint
and detekt write. Lines and columns count from 1; `ignore` severities are
left out; the code is the rule's short name (`WhitespaceAround` for
`…WhitespaceAroundCheck`).

**`json`:** a declared shape. Paths are dotted (`message.spans`); `[]`
walks an array, `[key=value]` takes the first element whose `key` is
`value`, and `../` climbs to the item the current one was found in.

| Field                            | Meaning                                                                                       |
| -------------------------------- | --------------------------------------------------------------------------------------------- |
| `lines`                          | One JSON document per line (`cargo --message-format=json`).                                   |
| `each`                           | The walk from a document to each problem (`comments[]`); the document itself when empty.      |
| `when`                           | Fields an item must have to count: `{"reason": "compiler-message"}`.                          |
| `path`, `line`, `message`        | Where the file, line and message are. Required.                                               |
| `column`, `endLine`, `endColumn` | The rest of the range, when the tool gives it.                                                |
| `severity`, `code`               | The severity word and the rule.                                                               |
| `severities`                     | The tool's severity words (or numbers, as text) to `error`, `warning`, `information`, `hint`. |
| `columnBase`                     | `1` (default) when columns count from 1, `0` when from 0.                                     |

cargo clippy's, for example:

```json
"output": {
	"format": "json",
	"json": {
		"lines": true,
		"when": { "reason": "compiler-message" },
		"path": "message.spans[is_primary=true].file_name",
		"line": "message.spans[is_primary=true].line_start",
		"column": "message.spans[is_primary=true].column_start",
		"severity": "message.level",
		"code": "message.code.code",
		"message": "message.message",
		"severities": { "note": "information", "help": "hint" }
	}
}
```

**`pattern`:** one problem per matching line, like VS Code's problem
matchers. `regexp` is a regular expression (Rust's `regex` syntax) with
the named groups `file` and `line` (required), and `column`, `endLine`,
`endColumn`, `severity`, `code` and `message`. `severity` gives the
severity when the line names none (otherwise `error`), and `severities`
maps the tool's words. Without a `message` group the whole line is the
message.

Common severity words are understood without a mapping: `error`, `err`,
`fatal`; `warning`, `warn`; `info`, `note`; `hint`, `help`, `style`.

## Adding a tool

1. Write `tools/registry/<id>.json`, starting from a record of the same
   kind and source. Take root markers and the run command from the tool's
   own documentation; Helix's `languages.toml` and nvim-lspconfig are good
   references. Settings copied from either must be credited in
   `THIRD_PARTY.md` (see [Review](#review)).
2. Leave `version` and `sha256` to the update script:
   `make tools-update ONLY=<id>` fills them in.
3. Add the ID to the languages' `tools` in `apps/ui/languages/languages.json`,
   where it belongs in their order. A tool that needs the project's
   configuration gets `configFiles`, so it does not run everywhere.
4. Run the server's tests (`make test`): every record must read, be
   valid, record its licence and source, be named after its ID, and every
   language must list only known tools.
5. Try it: `ahab lsp install <id>` on a machine, open a file, and check
   the file window's status and `ahab lsp status`.

## Updating versions

```sh
make tools-update                     # every record
make tools-update ONLY=gopls,biome    # some
```

The script (`scripts/tools-update.mjs`) asks each source for its newest
version (the npm registry, PyPI, crates.io, the Go proxy, GitHub
releases), pins it in the record, and records the SHA-256 of every release
file: from GitHub's digest when it gives one, otherwise by downloading
the file. npm packages in `extra` stay within their major version. A
`cargo` record with `git` takes the repository's latest release tag.
`download` sources are found per tool: today only jdtls's (Eclipse's
milestones). GitHub's API is used with `GITHUB_TOKEN`, or `gh auth token`,
when there is one. `node scripts/tools-update.mjs --dry-run` changes
nothing.

It lists what changed and what it could not update (a release missing a
platform's file, say: that asset loses its `sha256` and the platform
cannot install until it is fixed), and exits 1 when something needs a
look. The change is left for review; nothing is committed.

## Review

A record change is reviewed like code:

- the version is a release, not a pre-release, and the diff of checksums
  matches the platforms that changed;
- the `license` is the tool's, and the tool may be fetched and run as the
  record does;
- settings or run commands taken from Helix (MPL-2.0) or nvim-lspconfig
  (Apache-2.0) are credited in `THIRD_PARTY.md`;
- a new default does not surprise: a tool that needs the project's
  configuration has `configFiles`, a slow check does not run on save.

## Inside Ahab

For contributors to the server and the daemon:

- The server sends a daemon the records (Ahab's and enabled plugins'),
  each language's tools and the machine's and project's settings; the
  daemon adds the repository's `.ahab/tools.json`, finds tools and starts
  them.
- Daemon methods, announced in the capability handshake: `lsp.connect` (a
  window's channel to the language servers), `tools.status`,
  `tools.servers`, `tools.restart`, `tools.log`, `tools.install` and
  `tools.check`. The server never asks a daemon for a method it did not
  list; its answer is `machine_outdated`.
- The migration `20261117000008_language_tools.sql` adds `tool_settings`
  (per machine and project), `project_diagnostics` (the latest problems
  per project, file and tool) and `turn_diagnostics` (what a chat turn
  changed), each JSON column with its `_format`.
