---
title: Manifest reference
description: Every field of ahab-plugin.json, with its type, whether it is required, and what Ahab checks.
---

Every plugin has an `ahab-plugin.json` at the top of its folder. Ahab reads
and checks it when the plugin is installed or updated. A manifest that
breaks a rule is refused, with every problem listed. An unknown field is
kept and reported as a warning.

```json
{
	"id": "docker",
	"name": "Docker",
	"version": "0.1.0",
	"description": "Containers, images and logs on your machines; shells in containers.",
	"ahab": ">=0.1.0 <0.2.0",
	"entries": {
		"web": "dist/web.js",
		"daemon": "dist/daemon.js"
	},
	"hooks": ["terminals.create"],
	"machines": "all",
	"settings": "dist/settings.json"
}
```

## Fields

| Field         | Type                  | Required | Meaning                                                                  |
| ------------- | --------------------- | -------- | ------------------------------------------------------------------------ |
| `id`          | string                | yes      | The plugin's ID. It prefixes everything the plugin defines.              |
| `name`        | string                | yes      | The name people see. Must not be empty.                                  |
| `version`     | string                | yes      | The plugin's own version, in semver (`0.1.0`).                           |
| `description` | string                | no       | One line, shown in the plugin list.                                      |
| `ahab`        | string                | yes      | The plugin API versions it works with, as a version range.               |
| `entries`     | object                | yes\*    | The built files of its entries: `web`, `daemon`, `server`. At least one. |
| `hooks`       | array of strings      | no       | The hooks it intercepts. Default `[]`.                                   |
| `machines`    | `"all"` or `"chosen"` | no       | Where its daemon entry runs. Default `"all"`.                            |
| `settings`    | string or object      | no       | Its settings' JSON Schema, or the path of a file holding it.             |
| `dependsOn`   | object                | no       | Other plugins it uses, by ID, each with a version range. Default `{}`.   |
| `secrets`     | array of strings      | no       | Names of secrets it asks for. Planned; not used yet. Default `[]`.       |
| `contributes` | object                | no       | What it adds by declaring it: `languages`, `tools`. Experimental.        |

\* A plugin that only [contributes](#contributes) may leave out `entries`.

### `id`

Lowercase letters, digits and dashes; starts with a letter, does not end
with a dash; 2 to 40 characters; not `ahab` or `terminals`, which Ahab
keeps for itself. The ID names the plugin on the command line
(`ahab plugins enable docker`) and prefixes its commands and events
(`docker.containers.list`, `docker.container.changed`). It cannot change
between versions: updating a plugin from a folder that now holds another ID
is refused.

### `version`

A semver version such as `0.1.0` or `1.2.0-beta.1`. Ahab shows it; it does
not compare versions when updating.

### `ahab`

The range of the **plugin API** versions the plugin works with. Ahab
refuses the plugin when its own plugin API version is not in the range.
This is the plugin API version, not Ahab's release version: today it is
`0.1.3`. See [Versioning and compatibility](/docs/plugins/versioning/).

Ranges are read as npm or Rust write them:

| Range             | Means                                   |
| ----------------- | --------------------------------------- |
| `>=0.1.0 <0.2.0`  | 0.1.0 up to, not including, 0.2.0       |
| `>=0.1.0, <0.2.0` | the same, written as in Rust            |
| `^0.1`            | the same                                |
| `0.1.0`           | the same: a bare version means `^0.1.0` |
| `>=0.1.0`         | 0.1.0 and anything later                |

Use `>=0.1.0 <0.2.0` for now: while the API is below 1.0, a new minor
version may break plugins.

### `entries`

An object with up to three keys. Each value is a path inside the plugin's
folder (no `..`, no absolute paths). The files must exist when the plugin is
installed: build first.

| Key      | Status  | What it names                                                                                                      |
| -------- | ------- | ------------------------------------------------------------------------------------------------------------------ |
| `web`    | works   | The web entry's ES module, `dist/web.js` by convention. A stylesheet beside it (`dist/web.css`) is loaded with it. |
| `daemon` | works   | The daemon entry's ES module, `dist/daemon.js` by convention, run with Node on each machine.                       |
| `server` | planned | A server entry. Accepted with a warning; it does not run yet.                                                      |

`ahab-plugin build` builds `src/web/index.tsx` to the `web` path and
`src/daemon/index.ts` to the `daemon` path.

The manifest also accepts, for an entry, an object of paths by platform
(`{ "linux-x86_64": "bin/plugin" }`), meant for entries written in Rust.
That is planned: `ahab-plugin build` refuses it, and Ahab's daemon runs
only Node entries today.

### `hooks`

The hooks the plugin intercepts. Today the only hook is
`terminals.create`; any other name is refused. Ahab shows the list before
the plugin is enabled, and in Settings → Plugins. See
[Hooks](/docs/plugins/daemon-entry/#hooks).

### `machines`

Where the daemon entry runs:

- `"all"`: on every machine.
- `"chosen"`: only on the machines you pick, with
  `ahab plugins enable <id> --machine <name>` or in Settings → Plugins.

### `settings`

The plugin's settings, as a JSON Schema of `"type": "object"`. Usually a
path to the file the build writes, `"dist/settings.json"`; Ahab reads the
file into the manifest when installing, and refuses the plugin when it is
missing or is not an object schema. A schema written inline also works.
See [Settings, secrets and storage](/docs/plugins/settings/).

### `dependsOn`

Other plugins this one uses, by ID, each with a version range:
`{ "docker": ">=0.1.0 <0.2.0" }`. Ahab checks that each range is valid and
shows the list. A plugin is enabled only while each plugin it depends on
is installed, enabled and in range; otherwise enabling it is refused with
`plugin_dependency_missing`, which says what is missing. A plugin that
enabled plugins depend on cannot be disabled (`plugin_required`) until
they are.

### `secrets`

Planned, with server entries. Names listed here are accepted with a
warning; Ahab does not ask for or store them yet.

### `contributes`

:::caution[Experimental]
Contributions are new in plugin API 0.1.1 (`languages`) and 0.1.2
(`tools`, and languages' `tools`), and may change in any 0.x release. A
plugin that uses them should ask for the version that added them:
`>=0.1.1 <0.2.0` for languages, `>=0.1.2 <0.2.0` for tools, `>=0.1.3
<0.2.0` for Checkstyle XML or report files. An older Ahab ignores what it
does not know with a warning.
:::

What the plugin adds without code: **languages** (syntax highlighting)
and **tools** (language servers, formatters, linters and checks). A plugin may contribute only these, with no entries and nothing
to build. Another key in `contributes` is kept and reported as a
warning.

#### `contributes.languages`

A list of language declarations, in the format Ahab's own bundled
languages use. A language whose `id` is a bundled language's replaces it;
one with a new `id` adds a language. Where file patterns overlap, the
language of the plugin enabled last wins, and a person can still choose a
file's language, for one window or for every file of its pattern.

```json
{
	"id": "zig",
	"name": "Zig",
	"version": "0.1.0",
	"ahab": ">=0.1.1 <0.2.0",
	"contributes": {
		"languages": [
			{
				"id": "zig",
				"name": "Zig",
				"files": { "extensions": ["zig", "zon"], "names": [], "shebangs": [] },
				"aliases": ["zig"],
				"comments": { "line": "//" },
				"grammar": { "wasm": "grammars/tree-sitter-zig.wasm" },
				"queries": {
					"highlights": "queries/zig/highlights.scm",
					"injections": "queries/zig/injections.scm",
					"folds": "queries/zig/folds.scm",
					"tags": "queries/zig/tags.scm"
				}
			}
		]
	}
}
```

| Field      | Type   | Required | Meaning                                                                                                                                                                |
| ---------- | ------ | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`       | string | yes      | Lowercase letters, digits and `. _ + # -`, starting with a letter; at most 40. Unique in the plugin.                                                                   |
| `name`     | string | yes      | The name people see, as in the file window's language menu.                                                                                                            |
| `files`    | object | no       | How files are recognised: `extensions` (without the dot), `names` (exact, or with `*`: `Dockerfile.*`), `shebangs` (interpreters a `#!` line names). Lists of strings. |
| `aliases`  | array  | no       | Other names, as Markdown code fences and injections write them (`py` for Python).                                                                                      |
| `comments` | object | no       | `line` and `block` (`["/*", "*/"]`) comment markers.                                                                                                                   |
| `grammar`  | object | no       | `wasm`: the Tree-sitter grammar compiled to WebAssembly, plain or gzipped. Without it the language is plain text.                                                      |
| `queries`  | object | no       | Tree-sitter queries, each a path: `highlights`, `injections`, `folds`, `tags`.                                                                                         |
| `hidden`   | bool   | no       | Only used inside other languages (as `markdown.inline`); not offered in the menu.                                                                                      |
| `tools`    | array  | no       | The tool IDs that serve it, in order of preference (plugin API 0.1.2). Replaces the language's list when the `id` is a bundled language's.                             |

Paths are inside the plugin's folder. Ahab checks that each file exists
when the plugin is installed, and installs exactly the files named, from
wherever they are. Other fields are kept as written.

- **Files are recognised** by extension, then exact name, then the
  interpreter a `#!` line names, then an editor modeline (`vim: ft=zig`).
- **The grammar** is any Tree-sitter grammar built with `tree-sitter build
  --wasm` (tree-sitter 0.25 or later; language ABI 13 to 15). It loads in the
  browser, in a worker, the first time a file needs it.
- **Highlight captures** use Helix's names (`keyword`, `function.method`,
  `string`, `type.builtin`, `comment`…): Ahab's themes colour those, and
  Helix's queries can be used as they are. Later patterns win over earlier
  ones for the same node, as in Helix. A capture starting with `_` is not
  shown.
- **Injections** use `@injection.content` with `@injection.language` or
  `(#set! injection.language "…")`, as Helix and Neovim write them; the
  language is found by ID, alias or extension.
- **Folds** are the nodes captured `@fold`; without a folds query, a file
  folds at its multi-line blocks.
- **Tags** (`@definition.function`, `@reference.call`… with `@name`) give
  the file window's outline, and go to definition before a language server
  runs.

A grammar or query that fails to load leaves the language's files plain;
a query that does not fit the grammar is left out and the others still
work.

#### `contributes.tools`

Tool records, in the format of Ahab's own [tool registry](/docs/development/tool-registry/):
language servers, formatters, linters and check commands (plugin API
0.1.2; Checkstyle XML and report files 0.1.3). A record with a new `id` adds a tool: it serves the languages it
names, after their own tools. A record whose `id` is a bundled tool's, or
that names one in `replaces`, takes that tool's place in every language
that lists it.

```json
{
	"id": "zig",
	"name": "Zig",
	"version": "0.1.0",
	"ahab": ">=0.1.2 <0.2.0",
	"contributes": {
		"languages": [
			{
				"id": "zig",
				"name": "Zig",
				"files": { "extensions": ["zig", "zon"] },
				"grammar": { "wasm": "grammars/tree-sitter-zig.wasm" },
				"tools": ["zls"]
			}
		],
		"tools": [
			{
				"id": "zls",
				"name": "zls",
				"kind": "languageServer",
				"languages": ["zig"],
				"source": { "type": "system" },
				"run": {
					"command": ["zls"],
					"rootMarkers": ["build.zig"]
				}
			}
		]
	}
}
```

A `system` source means Ahab finds the tool on the machine and never
installs it. With an installable `source` (npm, a GitHub release…), the
tool can be installed on machines like Ahab's own. A record that takes a
default's place names it: `"replaces": "vscode-eslint-language-server"`.

Each record needs an `id` (lowercase letters, digits, dots and dashes), a
`kind` and `run.command`; a manifest without them is refused. A
record that is otherwise invalid (a linter without `run.output`, say) is
left out when Ahab chooses tools. Every tool can still be turned off per
project or machine (see [LSPs and Problems](/docs/language-tools/#settings)).

A contributed language lists its tools with `tools`, as above, by record
ID: Ahab's or the plugin's own.

A linter or check declares its output's `format`: `sarif`, `checkstyle`
(Checkstyle XML, which Checkstyle, PMD, SpotBugs, ktlint and detekt
write), `json` with a field mapping, or `pattern`. With `reports`, it reads
report files after the command ends instead of its output (plugin API
0.1.3): globs relative to the root, in the declared format, only those
written while it ran. A Gradle plugin's check, say:

```json
{
	"id": "gradle-check",
	"name": "Gradle check",
	"kind": "check",
	"languages": ["java", "kotlin"],
	"source": { "type": "system" },
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

For problems that need code, a daemon entry has the
[diagnostics API](/docs/plugins/daemon-entry/#problems).

## What Ahab installs

Ahab copies these files of the plugin's folder into its data folder, keyed
by their digest (SHA-256):

- `ahab-plugin.json`;
- files at the top whose names start with `README`, `LICENSE`, `OVERVIEW`
  or `CHANGELOG`;
- every file in the folders the entries are in (`dist/`);
- the files its languages name (grammars and queries).

Anything else, such as `src/` or `node_modules/`, stays behind. A daemon
entry must therefore bundle everything it needs; `ahab-plugin build` does.
Settings → Plugins shows `OVERVIEW.md` on the plugin's page, or
`README.md` when there is no overview.
