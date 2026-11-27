# @ahab/plugin

The SDK for Ahab plugins (see the [Plugin SDK docs](https://useahab.com/docs/plugins/), or `apps/docs/src/content/docs/docs/plugins/` in this repository):

- `@ahab/plugin/shared`: `defineCommand`, `defineEvent`, `defineChannel`
  and zod, for the schemas both entries share.
- `@ahab/plugin/node`: `definePlugin` for a `daemon` entry, which speaks
  the protocol below over the process's stdin and stdout.
- `@ahab/plugin/web`: `definePlugin` for a `web` entry, and its types. In
  Ahab this module is Ahab's own, through the web app's import map.
- `ahab-plugin build [--watch]`: builds `src/web/index.tsx` and
  `src/daemon/index.ts` into the files the manifest's `entries` name.
  React, zod and the SDK are left to Ahab's import map in the web bundle;
  the daemon bundle holds everything it needs, so machines never run
  `npm install`. A web entry may not import a daemon entry's files, nor
  the reverse: share them through `src/shared`.
- Styling: a web entry uses Tailwind classes, as Ahab's own components do.
  The build compiles the classes its `src/` uses, against Ahab's theme
  (`@ahab/plugin/theme.css`: the colour roles, fonts and radii), into a
  stylesheet beside the bundle (`dist/web.js` gets `dist/web.css`), which
  Ahab loads with the entry. The classes apply only inside the plugin's
  own content (Ahab marks it with `data-ahab-plugin="<id>"`), never to
  Ahab's pages or other plugins. Only utilities: the page's base styles and
  Ahab's components (such as `pill`) are already there. Container queries
  (`@container`, `@md:`) suit plugin views, which live in sidebars, tiles
  and phones alike.

While working on a plugin, `ahab plugins dev ./my-plugin` (on the server's
host) installs and enables the folder, runs `ahab-plugin build --watch`, and
updates the plugin after every build: its daemon entry restarts and its web
entry reloads, without restarting Ahab. Ctrl+C stops watching.

Entries run on Node 24 or later. The test plugin in
`crates/ahab/tests/fixtures/plugins/echo` uses every part but problems;
`crates/ahab/tests/fixtures/plugins/notes` uses the diagnostics API.

## Notifications (plugin API 0.1.4)

A plugin declares the kinds it sends in its manifest, so each shows in
Settings → Notifications under the plugin and can be turned off; sending
one it did not declare fails with `validation_failed`. It may also declare
destinations, where Ahab delivers every notification while "Show them
outside the app" is on:

```json
{
	"notifications": [{ "kind": "pipeline.failed", "title": "Pipeline failed" }],
	"destinations": [{ "id": "ntfy", "name": "ntfy" }],
	"hooks": ["notifications.send"]
}
```

```ts
export default definePlugin((ahab) => {
	// Kept as gitlab.pipeline.failed; null when it was not kept.
	const { id } = await ahab.notifications.send({
		kind: "pipeline.failed",
		title: "Pipeline failed · main",
		level: "error",
		target: { type: "url", url: "https://gitlab.example/p/1" },
		groupKey: "pipeline:main",
	});

	ahab.notifications.addDestination({
		id: "ntfy",
		async deliver(notification, { settings }) {
			// notification.url opens it in Ahab when the server's public_url is set.
			await fetch(`${settings.server}/${settings.topic}`, { method: "POST", body: notification.title });
		},
	});

	// Every notification, Ahab's too, before it is kept: change its text or drop it.
	ahab.hooks.before("notifications.send", (notification) =>
		notification.kind === "sessions.finished" && notification.title.includes("draft")
			? { action: "deny", reason: "Drafts are quiet." }
			: { action: "allow" },
	);

	ahab.events.on("notification.created", (notification) => ahab.log.info(notification));
});
```

A target is `{ type: "route", path }` (a page of Ahab), `{ type: "url",
url }` or `{ type: "window", window, props }`. A group key is the
plugin's own: it never replaces Ahab's notifications or another plugin's.
A hook may change `title`, `body`, `level` and `target`, never `kind` or
`source`. Each notification is delivered by the plugin on one machine it
runs on; a destination throws to say a delivery failed.

## Problems (plugin API 0.1.3)

A daemon entry keeps collections of problems, which show in Problems, the
editor and a turn's difference with the source `<plugin>.<collection>`:

```ts
import { definePlugin } from "@ahab/plugin/node";

export default definePlugin((ahab) => {
	const notes = ahab.diagnostics.collection("notes");
	ahab.diagnostics.onCheck(({ project, roots, reason }) => {
		for (const root of roots)
			notes.set(project, root ? `${root}/NOTES.md` : "NOTES.md", [
				{
					range: { start: { line: 0 } },
					severity: "information",
					message: `checked for ${reason}`,
				},
			]);
	});
});
```

Paths are relative to the project; others are dropped. A collection is
cleared when the plugin stops. See the daemon entry docs ("Problems").

## The protocol

A `daemon` (or, later, `server`) entry is a child process. It speaks
JSON-RPC 2.0 on stdin and stdout, framed as language servers do, with one
more header for channel bytes:

```text
Content-Length: <n>\r\n
Ahab-Channel: <uuid>\r\n      only for a channel's bytes
\r\n
<n bytes>
```

A frame without `Ahab-Channel` is one JSON-RPC message in UTF-8; a frame
with it is raw bytes for that channel. stdout is only the protocol: the
SDK sends the console to Ahab's logs. stderr is kept as logs too.

### Ahab to the plugin

| Method                  | Kind         | Params                                                 | Answer                                                                                                                                              |
| ----------------------- | ------------ | ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `initialize`            | request      | `plugin`, `machine`, `ahabVersion`, `settings`         | `protocol`, `commands` (name, description, input and output as JSON Schema), `channels` (name, params), `hooks`, `events` (the names it listens to) |
| `commands.run`          | request      | `name`, `input`, `context` (`project`: `id`, `folder`) | the output                                                                                                                                          |
| `$/cancelRequest`       | notification | `id` of a `commands.run`                               | –                                                                                                                                                   |
| `channels.open`         | request      | `name`, `channel` (its ID), `params`                   | `{}`; bytes follow on the channel                                                                                                                   |
| `channels.close`        | notification | `channel`                                              | –                                                                                                                                                   |
| `hooks.before`          | request      | `hook`, `data`                                         | `{ "action": "allow" }`, `{ "action": "change", "data": … }` or `{ "action": "deny", "reason": … }`                                                 |
| `events.deliver`        | notification | `name`, `data`                                         | –                                                                                                                                                   |
| `settings.changed`      | notification | `settings`                                             | –                                                                                                                                                   |
| `diagnostics.check`     | request      | `project` (`id`, `folder`), `roots`, `reason`          | `{}` once checked (0.1.3; `initialize`'s answer says `diagnostics.check`)                                                                           |
| `notifications.deliver` | request      | `destination`, `notification`, `settings`              | `{}` once delivered; an error says it failed (0.1.4)                                                                                                |
| `shutdown`              | request      | –                                                      | `{}`, then the process exits                                                                                                                        |

Every request but `initialize` waits until the plugin's function has run.

### The plugin to Ahab

| Method                | Kind         | Params                                                                                                                      |
| --------------------- | ------------ | --------------------------------------------------------------------------------------------------------------------------- |
| `log`                 | notification | `level` (`info`, `warn`, `error`), `message`                                                                                |
| `events.publish`      | notification | `name`, `data`, `lasting`                                                                                                   |
| `events.subscribe`    | notification | `names`: every event it listens to, when that changes after `initialize`                                                    |
| `channels.close`      | notification | `channel`, `reason` (optional)                                                                                              |
| `progress`            | notification | `id` of the `commands.run`, `value` (a fraction or a sentence)                                                              |
| `commands.call`       | request      | `name` (full: `docker.containers.list`, `terminals.create`), `input`, `machine` or `project`                                |
| `diagnostics.set`     | notification | `collection`, `project`, `files` (path → problems) (0.1.3)                                                                  |
| `diagnostics.delete`  | notification | `collection`, `project`, `paths` (0.1.3)                                                                                    |
| `diagnostics.clear`   | notification | `collection`, `project` (optional: every one) (0.1.3)                                                                       |
| `diagnostics.declare` | notification | `check`: whether it handles `diagnostics.check`, when that changes (0.1.3)                                                  |
| `notifications.send`  | request      | `kind` (declared, without the plugin's ID), `title`, `body`, `level`, `project`, `target`, `groupKey`; answers `id` (0.1.4) |

### Ahab's commands for plugins

A daemon entry calls these with `commands.call` (the SDK's
`ahab.terminals`); a plugin sees and stops only the terminals it started.

- `terminals.create`: `project`, and `title`, `command`, `args` (quoted
  for the shell after `command`), `cols`, `rows`; answers `session` and
  `terminal`.
- `terminals.list`: the plugin's terminals.
- `terminals.kill`: `terminal`.

Any other name is a plugin's command (`docker.containers.list`), run where
`machine` or `project` says, else wherever that plugin runs.

A request Ahab no longer wants (its caller went away) is followed by
`$/cancelRequest` with its `id`; the SDK aborts the handler's `signal`.

### Errors

Errors are JSON-RPC error objects with code `-32000` (or JSON-RPC's own),
a `message` for people, and `data.code` for programs: `command_unknown`,
`channel_unknown`, `invalid_input`, `command_failed`, `method_unknown`.
