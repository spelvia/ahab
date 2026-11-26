---
title: Quick start
description: Create, build and develop a small Ahab plugin with a web entry and a daemon entry.
---

This page builds **Hello**: a sidebar view with a button that runs a
command on the project's machine and shows its answer.

## Before you start

- Ahab running, with your computer as a machine (`ahab up`). The
  `ahab plugins` commands talk to the server the CLI is set up for.
- Node 24 or later, on the server's host and on every machine that runs
  daemon entries. Ahab uses `node` on `PATH`, or the one set in
  `config.toml`:

  ```toml
  [plugins]
  node = "/path/to/node"
  ```

- The SDK, `@ahab/plugin`. It is not published to npm yet: use it from a
  checkout of Ahab's repository whose dependencies are installed
  (`make setup`).

## Create the plugin

Make a folder with this layout:

```text
hello/
  ahab-plugin.json
  package.json
  src/shared/index.ts
  src/shared/settings.ts
  src/daemon/index.ts
  src/web/index.tsx
```

There is no project generator yet; `ahab-plugin` has only `build`.

`package.json` points at the SDK in your Ahab checkout. React is shared
with Ahab at run time; installing it here only gives the editor its types.

```json
{
	"name": "ahab-hello",
	"version": "0.1.0",
	"private": true,
	"type": "module",
	"scripts": {
		"build": "ahab-plugin build"
	},
	"dependencies": {
		"@ahab/plugin": "file:../ahab/sdk/plugin"
	},
	"devDependencies": {
		"@types/react": "^19.3.0",
		"react": "^19.3.1"
	}
}
```

`ahab-plugin.json` is the [manifest](/docs/plugins/manifest/):

```json
{
	"id": "hello",
	"name": "Hello",
	"version": "0.1.0",
	"description": "Says hello from your machines.",
	"ahab": ">=0.1.0 <0.2.0",
	"entries": {
		"web": "dist/web.js",
		"daemon": "dist/daemon.js"
	},
	"settings": "dist/settings.json"
}
```

`src/shared/index.ts` defines the command both entries use:

```ts
import { defineCommand, z } from "@ahab/plugin/shared";

export const hello = defineCommand({
	name: "hello",
	target: "machine",
	description: "Says hello from the machine it runs on.",
	input: z.object({ name: z.string().default("world") }),
	output: z.object({ text: z.string() }),
});
```

`src/shared/settings.ts` defines the plugin's settings. Ahab shows a form
for them in Settings → Plugins:

```ts
import { defineSettings, z } from "@ahab/plugin/shared";

export const settings = defineSettings({
	greeting: z.string().default("Hello").describe("The word to greet with."),
});
```

`src/daemon/index.ts` answers the command on each machine:

```ts
import { definePlugin } from "@ahab/plugin/node";

import { hello } from "../shared/index.ts";

export default definePlugin((ahab) => {
	ahab.commands.handle(hello, ({ name }, call) => ({
		text: `${String(call.settings.greeting)}, ${name}, from ${ahab.machine}.`,
	}));
	ahab.log.info("hello is running on", ahab.machine);
});
```

`src/web/index.tsx` adds a sidebar view that calls it:

```tsx
import { Button, definePlugin, toast, toastError, useProject } from "@ahab/plugin/web";

import { hello } from "../shared/index.ts";

export default definePlugin((ahab) => {
	function HelloView() {
		const project = useProject();
		const say = () =>
			ahab.commands
				.call<{ text: string }>(hello, { name: "Ahab" }, { project: project?.id })
				.then((answer) => toast(answer.text), toastError);
		return (
			<div className="flex flex-col gap-2 p-3">
				<p className="text-muted-foreground text-sm">Ask this project's machine.</p>
				<Button size="sm" disabled={!project} onClick={() => void say()}>
					Say hello
				</Button>
			</div>
		);
	}

	ahab.views.add({
		id: "hello",
		title: "Hello",
		icon: <span aria-hidden>H</span>,
		Component: HelloView,
	});
});
```

## Build it

```sh
npm install
npx ahab-plugin build      # or: npm run build
```

The build writes `dist/`:

| File                 | What                                                                    |
| -------------------- | ----------------------------------------------------------------------- |
| `dist/web.js`        | the web entry, without React, zod or the SDK (Ahab provides them)       |
| `dist/web.css`       | the Tailwind classes `src/` uses, in Ahab's theme, scoped to the plugin |
| `dist/daemon.js`     | the daemon entry, with all its dependencies bundled                     |
| `dist/settings.json` | the settings' JSON Schema, from `src/shared/settings.ts`                |
| `dist/package.json`  | `{ "type": "module" }`, so Node reads the bundles as ES modules         |

Each entry also gets a source map. `ahab-plugin build --watch` rebuilds on
every change.

The build fails when the web entry imports a daemon entry's files or the
reverse; share code through `src/shared/`. If `src/shared/settings.ts`
exists but the manifest does not name `dist/settings.json`, it warns.

## Develop it

On the server's host, from the plugin's folder:

```sh
ahab plugins dev .
```

This builds the plugin, installs it (or updates it, if it was installed
from this folder before), enables it, and runs `ahab-plugin build --watch`.
After every build it updates the plugin: the daemon entry restarts on
every machine and the web entry reloads in open browsers, without
restarting Ahab. Ctrl+C stops watching; the plugin stays installed and
enabled.

The watching build writes the settings' schema only when it starts: after
changing `src/shared/settings.ts`, stop `ahab plugins dev` and run it again.

`ahab plugins dev` runs the build with `npx --no-install ahab-plugin`, so
install the plugin's dependencies first. With `--no-build`, it only
watches `dist/` and the manifest, for a plugin you build some other way.

Open a project in the web app: **Hello** is in the sidebar. Run the
command from the command line too:

```sh
ahab run hello                    # lists its commands and their fields
ahab run hello.hello name=you     # runs it
```

When the plugin runs on more than one machine, say which with
`--machine <name>` or `--project <id>`.

## Install it without dev mode

The folder must be on the server's host and already built:

```sh
ahab plugins add ./hello          # installs it, disabled
ahab plugins enable hello
ahab plugins update hello         # after a new build
```

`ahab plugins add` also takes a Git URL. The server clones the repository;
when the built files are not committed and there is a `package.json`, it
runs `npm ci` (or `npm install`) and `npm run build` with its Node, for at
most ten minutes.

## Other commands

| Command                                     | What it does                                                                    |
| ------------------------------------------- | ------------------------------------------------------------------------------- |
| `ahab plugins` (`list`, `ls`)               | The installed plugins, their state on each machine.                             |
| `ahab plugins show <id>`                    | One plugin: entries, hooks, state per machine with its message.                 |
| `ahab plugins enable <id> --machine <name>` | Enables it on the named machines (repeat the flag), for `"machines": "chosen"`. |
| `ahab plugins disable <id>`                 | Turns it off everywhere.                                                        |
| `ahab plugins logs <id> [--machine <name>]` | What it logged on a machine; the server's own machine by default.               |
| `ahab plugins remove <id> [--yes]` (`rm`)   | Removes a disabled plugin and its files.                                        |
| `ahab run <id>.<command> [field=value…]`    | Runs a command; `--input <json>`, `--machine`, `--project`, `--table`.          |
