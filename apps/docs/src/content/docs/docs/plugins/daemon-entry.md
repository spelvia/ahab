---
title: Daemon entry
description: The part of a plugin that runs on each machine, how Ahab starts it, and what it can do.
---

The daemon entry is the part of a plugin that runs on a machine: next to
its files, processes, Docker socket or kubeconfig. Each machine's daemon
runs it as a Node process and speaks with it over its standard input and
output.

## The entry

`src/daemon/index.ts` exports one function by default, wrapped in
`definePlugin` from `@ahab/plugin/node`. It registers what the plugin
handles; it may be `async`, and may return a cleanup function.

```ts
import { definePlugin } from "@ahab/plugin/node";

import { hello } from "../shared/index.ts";

export default definePlugin((ahab) => {
	ahab.commands.handle(hello, ({ name }) => ({ text: `Hello, ${name}.` }));
	return () => {
		// Runs when the plugin stops.
	};
});
```

Register commands, channels and hooks while this function runs. Ahab reads
what the plugin offers when the function has finished, and asks only
plugins that registered a hook then. Event listeners may be added at any
time.

## How it runs

- **Where:** on every machine when the manifest says `"machines": "all"`,
  or on the machines you chose for `"chosen"`; only while the plugin is
  enabled. The server sends each machine the plugin's build; machines
  never run `npm install`.
- **How:** `node --enable-source-maps dist/daemon.js`, in the build's
  folder in the machine's data folder, with Node 24 or later: the one set
  as `[plugins] node` in `config.toml`, else `node` on `PATH`. Without a
  suitable Node, the plugin is `unavailable` on that machine.
- **Environment:** a clean one. Only `PATH`, `HOME`, `USER`, `LANG`, `TZ`,
  `KUBECONFIG`, `DOCKER_HOST` and `XDG_RUNTIME_DIR` are passed on from the
  daemon, plus `AHAB_PLUGIN` (the plugin's ID) and `AHAB_MACHINE` (the
  machine's name).
- **Start:** the entry's function must finish within 10 seconds.
- **Crashes:** a process that exits is started again after 1, 2, 4, 8…
  seconds, up to 30. The fifth crash within two minutes marks it `failed`;
  it stays off until a new build arrives or it is enabled again.
- **Stop:** when the plugin is disabled, updated or no longer meant for the
  machine, Ahab asks it to shut down: open channels close, cleanups run (the
  last registered first), and the process exits. After 5 seconds without an
  answer, and 5 more to exit, it is killed.
- **Updates:** a new build stops the old process and starts the new one.

Its state on each machine is one of `receiving` (the build is on its way),
`starting`, `running`, `restarting`, `failed` and `unavailable`, with a
message. `ahab plugins` and `ahab plugins show <id>` list them; so does
Settings → Plugins.

## Logs

The process's standard output is the protocol, so **do not write to
`process.stdout`**. The SDK sends `console.log`, `console.info` and
`console.debug` to Ahab's logs as `info`, `console.warn` as `warn` and
`console.error` as `error`, as do `ahab.log.info`, `ahab.log.warn` and
`ahab.log.error`. Lines on standard error are kept too, marked
`[stderr]`.

The daemon keeps the last 1,000 lines per plugin, in memory: they are lost
when the daemon restarts. Read them with:

```sh
ahab plugins logs hello                  # the server's own machine
ahab plugins logs hello --machine atlas
```

## The context

### `plugin`, `machine`, `ahabVersion`

The plugin's ID, the machine's name, and the plugin API version of the
Ahab that started it (`0.1.3` today; not Ahab's release version).

### `commands.handle(definition, handler)`

Answers one of the plugin's commands. Ahab checks the input against the
definition's schema first, filling in defaults; invalid input never
reaches the handler. Return the output, or a promise of it.

```ts
ahab.commands.handle(listContainers, async ({ all }, call) => {
	const list = await containers(all, call.signal);
	return list;
});
```

The handler's second argument describes the call:

| Field      | Meaning                                                                                    |
| ---------- | ------------------------------------------------------------------------------------------ |
| `project`  | `{ id, folder }` when it was run for a project: the folder's path on this machine.         |
| `signal`   | An `AbortSignal`, aborted when the caller goes away (a closed page, Ctrl+C on `ahab run`). |
| `settings` | The settings in effect for this call: the project's when there is one, else the machine's. |
| `progress` | Reports how far along it is. Planned: Ahab ignores progress reports today.                 |

A command stops after ten minutes. The output is not checked against its
schema; keep it to what the schema says, since callers rely on it.

To fail, throw. The error's message reaches the caller with the code
`command_failed`. `PluginError(code, message)` adds a code of your own,
in `snake_case` (up to 64 characters): the API's answer keeps the code
`command_failed`, so callers can rely on Ahab's codes, and carries yours
as `pluginCode`. A few codes keep the meaning Ahab gives them:
`invalid_input` becomes `validation_failed` (422) and `command_unknown`
stays `command_unknown` (404).

```ts
import { PluginError } from "@ahab/plugin/node";

throw new PluginError("invalid_input", "That is not a container name.");
```

`definition.target` is `"machine"` for every command today. `"server"`
(commands answered by a server entry) is planned.

### `commands.call(name, input, target?)`

Runs another command through Ahab, by its full name, and resolves with
its output: another plugin's (`"docker.containers.list"`) or one of Ahab's
commands for plugins (see [Terminals](#terminals)). `target` is
`{ machine }` or `{ project }`, as in the web entry.

A daemon entry reaches only **its own machine**: without a target the
command runs here, and a machine or project elsewhere is refused with
`other_machine_not_allowed`, as are other machines' terminals. A
compromised machine therefore cannot reach the others through Ahab. To
work across machines, call commands from the web entry, which runs with
the person's client.

```ts
const containers = await ahab.commands.call("docker.containers.list", { all: true });
```

### `channels.handle(definition, handler)`

Answers a channel: a two-way byte stream with a web entry. The handler gets
the channel and its params, checked against the definition.

```ts
ahab.channels.handle(logs, (channel, { container, tail }) => {
	const child = spawn("docker", ["logs", "--follow", "--tail", String(tail), container]);
	child.stdout.on("data", (chunk: Buffer) => channel.write(new Uint8Array(chunk)));
	child.on("exit", () => channel.close("The container stopped."));
	channel.onClose(() => child.kill());
});
```

| Member              | What                                                           |
| ------------------- | -------------------------------------------------------------- |
| `id`                | The channel's ID.                                              |
| `write(data)`       | Sends bytes, or text as UTF-8.                                 |
| `onData(listener)`  | Calls `listener` with each chunk of bytes the web entry sends. |
| `onClose(listener)` | Calls `listener` once the channel closed, from either side.    |
| `close(reason?)`    | Closes it; the web entry gets the reason.                      |

When the handler throws, the channel closes with the error's message. A
reader that falls too far behind (512 chunks) ends the channel. A plugin
that stops closes all its channels.

### Hooks

`hooks.before(hook, handler)` intercepts something before it happens. The
handler answers one of:

- `{ action: "allow" }`: let it be;
- `{ action: "change", data }`: change it;
- `{ action: "deny", reason }`: refuse it, with a reason people see.

The one hook today is **`terminals.create`**: a terminal about to start on
this machine. List it in the manifest's `hooks` as well: Ahab shows that
list before the plugin is enabled, and asks a plugin only about the hooks
it lists. A hook registered but not listed is logged and never asked. A
hook may be registered at any time, not only when the plugin starts.

| Field     | Type              | Meaning                                                                                   |
| --------- | ----------------- | ----------------------------------------------------------------------------------------- |
| `command` | string or null    | What runs; the login shell when null.                                                     |
| `env`     | object of strings | Variables to add. Empty when the hook is called; names starting with `AHAB_` are ignored. |
| `folder`  | string            | The folder it starts in. A folder that does not exist is ignored.                         |
| `context` | object            | What the starter said it is for, by plugin: `{ "docker": { "container": "web" } }`.       |

```ts
ahab.hooks.before("terminals.create", (data) => {
	const wanted = data.context.docker as { container?: string } | undefined;
	if (!wanted?.container) return { action: "allow" };
	return {
		action: "change",
		data: { ...data, command: `docker exec -it ${wanted.container} sh` },
	};
});
```

How interceptors run:

- Every running plugin that registered the hook is asked, one after
  another, in the interceptors' order: the order plugins were installed
  in, unless you change it in Settings → Plugins.
- Each sees the changes made before it.
- A plugin that does not answer within 5 seconds, or fails, is passed over.
- A denial stops there: the terminal does not start, and the error names
  the plugin and its reason.
- The terminal records the plugins that changed it. Restarting it runs what
  the hooks made, without asking again.

Pass `context` when starting a terminal from the web entry
(`ahab.terminals.start`) to ask for a hook's change. Only act on context
your plugin owns, and check values before putting them on a command line.

More hooks (chats, permission requests, files, Git, commands) and observing
hooks are planned.

### Events

`events.publish(definition, data)` publishes one of the plugin's events.
The data is checked against the definition's schema; invalid data throws.
Other entries and plugins hear it as `<plugin>.<name>`.

```ts
export const containerChanged = defineEvent({
	name: "container.changed",
	data: z.object({ id: z.string(), action: z.string() }),
});

ahab.events.publish(containerChanged, { id, action: "start" });
```

Events are live: they are not stored, and only those listening at that
moment hear them. An event defined with `lasting: true` is kept while the
machine is cut off from the server, and sent once it is back.

`events.on(name, listener)` listens to any event: Ahab's own
(`terminal.exited`, `machine.online`…, listed under
[Web entry](/docs/plugins/web-entry/#eventsonname-listener)) and every
plugin's, by full name. It returns a function that stops listening.

```ts
ahab.events.on("terminal.exited", (data) => ahab.log.info("a terminal exited", data));
```

### Terminals

`ahab.terminals` starts terminals in Ahab, in a project on the plugin's
own machine. They show in the web app like any other terminal. A plugin
sees and stops only the terminals it started on its machine.

```ts
const { session, terminal } = await ahab.terminals.create({
	project: call.project.id,
	title: "Port-forward web",
	command: "kubectl",
	args: ["port-forward", "svc/web", "8080:80"],
});
await ahab.terminals.list(); // this plugin's terminals
await ahab.terminals.kill(terminal.id);
```

| Option         | Meaning                                                       |
| -------------- | ------------------------------------------------------------- |
| `project`      | Required: the project's ID. The terminal opens in its folder. |
| `title`        | The terminal's title.                                         |
| `command`      | Run by the login shell; the shell itself when absent.         |
| `args`         | Words after `command`, each quoted for the shell.             |
| `cols`, `rows` | Its size; 120 by 32 by default.                               |

A terminal is listed with `id`, `sessionId`, `projectId`, `command`,
`state` (`running` or `exited`), `exitCode` and `title`. These are calls to
Ahab's commands `terminals.create`, `terminals.list` and `terminals.kill`.

### Settings

`ahab.settings()` returns the settings in effect on this machine: the
schema's defaults, then the server's values, then the machine's.
`ahab.onSettings(listener)` calls `listener` with the new values when they
change. A command run for a project gets that project's values in
`call.settings`. See [Settings, secrets and storage](/docs/plugins/settings/).

```ts
const docker = () => String(ahab.settings().command ?? "docker");
ahab.onSettings((settings) => ahab.log.info("settings changed", settings));
```

### Problems

:::note[Plugin API 0.1.3]
New in plugin API 0.1.3: set `"ahab": ">=0.1.3 <0.2.0"` in the manifest
to use it. Experimental, like the rest of the plugin API.
:::

A plugin that only wraps a command's output needs no code for problems: it
contributes a [tool record](/docs/plugins/manifest/#contributestools) with its
output format (SARIF, Checkstyle XML, JSON, a line pattern) and report
files. For problems that need real work (a build tool's structured
output, a tool behind a server, or problems that are not about code at
all), a daemon entry keeps **collections** of them, as VS Code's
`DiagnosticCollection`:

```ts
import { definePlugin } from "@ahab/plugin/node";

export default definePlugin((ahab) => {
	const problems = ahab.diagnostics.collection("gradle");

	// Check project, and after agents' turns: check what Ahab asks for.
	ahab.diagnostics.onCheck(async ({ project, roots, reason }) => {
		for (const root of roots) {
			const found = await runGradle(project.folder, root); // your own
			problems.set(project, found); // { "app/src/Main.java": [ … ], … }
		}
	});

	// Or whenever something happens: a file watcher, a build event.
	problems.set(projectId, "app/src/Main.java", [
		{
			range: { start: { line: 11, character: 4 }, end: { line: 11, character: 20 } },
			severity: "warning",
			message: "getFoo() is deprecated",
			code: "deprecation",
		},
	]);
	problems.delete(projectId, "app/src/Old.java");
	problems.clear(projectId); // or clear() for every project
});
```

- **One form.** A problem has a `range` (lines and characters from 0, as
  in LSP; `end` defaults to `start`), a `severity` (`error`, the default,
  `warning`, `information`, `hint`), a `message`, an optional `code`, and
  optional `fixes`: each a `title` and `edits` to the problem's file
  (`{ range, newText }`), offered in the editor as quick fixes (hover the
  problem, or ⌘. on its line) in Edit mode and applied as unsaved changes.
  They show in Problems, in the editor and in a turn's difference like
  every other source's.
- **Their source** is `<plugin>.<collection>` (`ahab-gradle.gradle`),
  filterable in Problems and turned off per project like a tool (from
  Problems' funnel, or `tools.<source>.off` in the project's settings).
- **Paths** are relative to the project (absolute ones inside its folder
  are accepted). Any other path, and any project not on this machine, is
  dropped and logged.
- `set(project, path, problems)` replaces what the collection said of
  that file; `set(project, { path: problems, … })` several at once;
  `delete` forgets a file's; `clear` a project's, or every one's. An
  empty list clears a file. A project is its ID, or the `project` a
  check gets.
- **`onCheck(handler)`** runs for Check project and after an agent's turn
  (not on save), alongside the check commands. `roots` are the
  repositories and worktrees in scope, as folders relative to the
  project (`""` is its own folder); `reason` is `request` or
  `agentTurn`. Ahab waits for the handler (15 minutes at most); a handler
  that throws is logged. It returns a remover.
- A collection's name is lower-case letters, digits and dashes. Its
  problems are **cleared when the plugin stops**, crashes or is disabled.

### `onStop(listener)` and `log`

`onStop` runs `listener` when the plugin stops, after its channels have
closed; listeners run last registered first, and may be `async`. `log.info`,
`log.warn` and `log.error` write to the plugin's logs.

## The protocol

The SDK speaks the protocol for you. It is described here for debugging,
and for entries in other languages (planned).

The process speaks **JSON-RPC 2.0** on its standard input and output,
framed as language servers do, with one extra header for a channel's
bytes:

```text
Content-Length: <n>\r\n
Ahab-Channel: <uuid>\r\n      only for a channel's bytes
\r\n
<n bytes>
```

A frame without `Ahab-Channel` is one JSON-RPC message in UTF-8; a frame
with it is raw bytes for that channel.

From Ahab to the plugin:

| Method              | Kind         | Params                                             | Answer                                                                         |
| ------------------- | ------------ | -------------------------------------------------- | ------------------------------------------------------------------------------ |
| `initialize`        | request      | `plugin`, `machine`, `ahabVersion`, `settings`     | `protocol`, `commands`, `channels`, `hooks`, `events`, `diagnostics` (`check`) |
| `commands.run`      | request      | `name`, `input`, `context` (`project`, `settings`) | the output                                                                     |
| `$/cancelRequest`   | notification | `id` of a `commands.run`                           |                                                                                |
| `channels.open`     | request      | `name`, `channel` (its ID), `params`               | `{}`; bytes follow                                                             |
| `channels.close`    | notification | `channel`                                          |                                                                                |
| `hooks.before`      | request      | `hook`, `data`                                     | `allow`, `change` or `deny`, as above                                          |
| `events.deliver`    | notification | `name`, `data`                                     |                                                                                |
| `settings.changed`  | notification | `settings`                                         |                                                                                |
| `diagnostics.check` | request      | `project` (`id`, `folder`), `roots`, `reason`      | `{}` once checked (0.1.3)                                                      |
| `shutdown`          | request      |                                                    | `{}`, then the process exits                                                   |

From the plugin to Ahab:

| Method                | Kind         | Params                                                     |
| --------------------- | ------------ | ---------------------------------------------------------- |
| `log`                 | notification | `level` (`info`, `warn`, `error`), `message`               |
| `events.publish`      | notification | `name`, `data`, `lasting`                                  |
| `events.subscribe`    | notification | `names`: every event it listens to, when that changes      |
| `hooks.declare`       | notification | `hooks`: every hook it intercepts, when that changes       |
| `channels.close`      | notification | `channel`, `reason` (optional)                             |
| `progress`            | notification | `id` of the `commands.run`, `value`; ignored today         |
| `commands.call`       | request      | `name` (full), `input`, `machine` or `project`             |
| `diagnostics.set`     | notification | `collection`, `project`, `files` (path → problems) (0.1.3) |
| `diagnostics.delete`  | notification | `collection`, `project`, `paths` (0.1.3)                   |
| `diagnostics.clear`   | notification | `collection`, `project` (optional) (0.1.3)                 |
| `diagnostics.declare` | notification | `check`: whether it handles `diagnostics.check` (0.1.3)    |

Errors are JSON-RPC error objects with code `-32000` (or JSON-RPC's own),
a `message` for people and `data.code` for programs: `command_unknown`,
`channel_unknown`, `invalid_input`, `command_failed`, `method_unknown`, or
the plugin's own.
