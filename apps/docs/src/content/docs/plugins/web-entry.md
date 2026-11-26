---
title: Web entry
description: What a plugin's web entry can add to Ahab's web app, the component kit, styling, and phones.
---

The web entry is an ES module the web app loads in the browser. It adds
views, windows and commands to the web app, built from Ahab's own
components and styled with Ahab's theme.

## The entry

`src/web/index.tsx` exports one function by default, wrapped in
`definePlugin`. Ahab calls it with the plugin's **context** once the module
loads:

```tsx
import { definePlugin } from "@ahab/plugin/web";

export default definePlugin((ahab) => {
	const removeView = ahab.views.add({/* … */});
	return () => {
		// Optional cleanup, when the plugin unloads.
	};
});
```

Every `add` returns a function that removes what it added. When the plugin
is disabled, removed or rebuilt, Ahab calls the cleanup, then removes
everything the plugin still had added. A rebuilt plugin (a new digest) is
loaded again in every open browser, without a page reload.

If the module fails to load, or its default export is not a function,
Settings → Plugins shows why. If one of its components throws, that box
shows the error and the rest of Ahab keeps working.

### What the build shares with Ahab

`ahab-plugin build` does not bundle these modules into `dist/web.js`;
the web app provides its own copies at run time:

- `react`, `react/jsx-runtime`, `react-dom`, `react-dom/client`
- `zod`
- `@ahab/plugin/shared`, `@ahab/plugin/web`

Every other package (icons such as `lucide-react`, your own code) is
bundled. A plugin's components are ordinary React components, and hooks
work as usual.

## The context

The examples below assume `ahab` is the context, and `hello`, `lines` and
`said` are definitions from `src/shared/`.

### `plugin` and `project()`

`ahab.plugin` is the plugin's ID. `ahab.project()` returns the project
open in the web app (`id`, `machineId`, `name`, `path`), or `undefined`.
Inside components, use the `useProject()` hook instead: it re-renders when
the project changes.

### `commands.call(command, input?, target?)`

Runs a command and resolves with its output. It rejects with the command's
error.

```ts
const answer = await ahab.commands.call<{ text: string }>(
	hello,
	{ name: "you" },
	{ project: ahab.project()?.id },
);
```

`command` is one of:

- a definition from `defineCommand`: this plugin's command;
- a full name, `"<plugin>.<command>"`: any plugin's command
  (`"docker.containers.list"`);
- a name without a dot: this plugin's command.

A string with a dot is always read as `<plugin>.<command>`. For this
plugin's own commands whose names contain dots (`containers.list`), pass
the definition or the full name.

`target` says where it runs: `{ machine: <machine ID> }`, or
`{ project: <project ID> }` (that project's machine; the command
also gets the project's folder and settings). Without a target, it runs
on the one machine the plugin runs on, and fails when the plugin runs on
several. Commands stop after ten minutes.

### `channels.open(channel, params?, target?)`

Opens a two-way byte stream with the plugin's daemon entry on a machine,
over a WebSocket. `channel` and `target` work as for commands; `params` is
checked against the channel's schema.

```ts
const channel = ahab.channels.open(lines, { prefix: "> " }, { machine });
const stopListening = channel.onData((bytes) => show(new TextDecoder().decode(bytes)));
channel.onClose((reason) => reason && toast(reason));
channel.write("hello"); // text or a Uint8Array; sent once the socket opens
channel.close();
```

`onClose` is called once, with the reason the daemon entry gave, or with
"The connection to the plugin was lost." when the connection broke.

### `events.on(name, listener)`

Calls `listener` with each event's data from now on; returns a function
that stops it. Events come from the server as they happen and are not
stored: a browser that was not listening misses them.

```ts
const stop = ahab.events.on("docker.container.changed", () => refresh());
```

A plugin's events are named `<plugin>.<name>`. Ahab's own:

| Event              | Data                                                                             |
| ------------------ | -------------------------------------------------------------------------------- |
| `machine.online`   | `machine` (ID), `name`                                                           |
| `machine.offline`  | `machine`                                                                        |
| `project.added`    | `project`, `machine`, `path`                                                     |
| `session.archived` | `session`, `project`                                                             |
| `terminal.exited`  | `terminal`, `session`, `project`, `machine`, `code`, `plugin` (who started it)   |
| `plugins.changed`  | `plugin`: installed, updated, enabled, disabled, removed or its settings changed |

### `views.add(view)`

A sidebar view, beside Sessions, Files, Git and Search.

```tsx
ahab.views.add({
	id: "containers",
	title: "Containers",
	icon: <ContainerIcon />,
	Component: ContainersList,
});
```

### `activity.add(item)`

An icon in the activity bar that opens one of the plugin's windows, or
brings it forward.

```tsx
ahab.activity.add({
	id: "board",
	title: "Board",
	icon: <LayoutIcon />,
	opens: { window: "board", params: {} },
});
```

### `windows.add(window)` and `windows.open(kind, params?)`

A kind of window in the tiles, beside sessions and files. Windows are kept
in the layout like Ahab's own. `title` makes the tab's title from the
window's params. With `single: true` there is one window of that kind:
opening it again brings it forward, with the new params.

```tsx
ahab.windows.add<{ name: string }>({
	kind: "logs",
	title: (params) => `Logs · ${params.name}`,
	Component: ({ params }) => <Logs name={params.name} />,
});

ahab.windows.open("logs", { name: "web" });
```

`open` opens only this plugin's kinds. Keep params small and plain: they
are saved with the layout.

### `newWindow.add(item)`

An entry in the **+** menus at the end of each tab bar.

```tsx
ahab.newWindow.add({
	id: "containers",
	title: "Containers…",
	icon: <ContainerIcon />,
	run: () => ahab.windows.open("containers"),
});
```

### `palette.add(item)`

A command in the command palette, under **Plugins**. `keywords` help it be
found.

```ts
ahab.palette.add({
	id: "containers",
	title: "Docker: Show containers",
	keywords: ["docker", "containers"],
	run: () => ahab.windows.open("containers"),
});
```

### `status.add(item)`

A small component in the status bar along the bottom of the workbench,
such as a count or the current context. Plugins' items follow Ahab's own
on the left (the machine, the branch, the problem counts), in the order
plugins load.

```ts
ahab.status.add({ id: "running", Component: RunningCount });
```

Ahab keeps each item as tall as the bar (24 pixels) and square-cornered,
whatever height its component draws, so a button made for the old title
bar still fits. Make it a button with a `title` that says what clicking
does, its text `text-xs`, and keep it short: a count or a word.

:::note[Before]
Items added with `status.add` showed in the title bar until the status bar
came. Nothing changes for a plugin: the same call puts them in the status
bar.
:::

### `settings.page(page)`

The plugin's own settings page, in Settings → Plugins, instead of the
generated form. See [Settings, secrets and storage](/docs/plugins/settings/).

```ts
ahab.settings.page({ Component: SettingsPage });
```

### `terminals.start(options)`

Starts a terminal in Ahab, in a project (the current one by default), and
opens it in a window. Plugins' `terminals.create` hooks see it before it
starts, with its `context`.

```ts
const { session, terminal } = await ahab.terminals.start({
	title: "Shell in web",
	context: { docker: { container: "web" } },
});
```

| Option    | Meaning                                                                    |
| --------- | -------------------------------------------------------------------------- |
| `project` | The project's ID; the current one when absent. Fails when there is none.   |
| `title`   | The terminal's title.                                                      |
| `command` | What runs; the login shell when absent.                                    |
| `context` | What it is for, by plugin (`{ docker: { container: "web" } }`), for hooks. |
| `show`    | `false` to not open a window, and show it yourself with `<Terminal id>`.   |

It resolves with the session's ID and the terminal's ID.

## The component kit

`@ahab/plugin/web` exports Ahab's own components and hooks, so a plugin
looks and behaves like the rest of Ahab. Import them from the SDK; the
web app supplies the real ones at run time (outside Ahab they throw).

| Export                                                                                                    | What                                                                                                                                             |
| --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `Button`                                                                                                  | Ahab's button: `variant` (`default`, `outline`, `ghost`, `destructive`, `secondary`, `link`), `size` (`default`, `sm`, `lg`, `icon`), `asChild`. |
| `IconButton`                                                                                              | A small square button with an icon; `label` is for screen readers.                                                                               |
| `CopyButton`                                                                                              | Copies `text`.                                                                                                                                   |
| `Dialog`, `DialogTrigger`, `DialogClose`, `DialogContent`, `DialogTitle`, `DialogDescription`             | A dialog.                                                                                                                                        |
| `DropdownMenu`, `DropdownMenuTrigger`, `DropdownMenuContent`, `DropdownMenuItem`, `DropdownMenuSeparator` | A menu from a button.                                                                                                                            |
| `ContextMenu`, `ContextMenuTrigger`, `ContextMenuContent`, `ContextMenuItem`                              | A right-click menu.                                                                                                                              |
| `ConfirmDialog`                                                                                           | Asks before something that cannot be undone.                                                                                                     |
| `NameDialog`                                                                                              | Asks for one line of text.                                                                                                                       |
| `Note`                                                                                                    | A sentence beside an icon, in a quiet box.                                                                                                       |
| `Markdown`                                                                                                | Markdown, as Ahab shows it in chats.                                                                                                             |
| `FileIcon`                                                                                                | A file's icon from the icon theme, by `path`.                                                                                                    |
| `Terminal`                                                                                                | A terminal Ahab runs, by its `id`, as in Ahab's terminal windows.                                                                                |
| `toast(message, { duration? })`, `toastError(error)`                                                      | A short message in the corner; an error's message, for longer.                                                                                   |
| `cn(...classes)`                                                                                          | Joins class names; later Tailwind classes win over earlier ones.                                                                                 |
| `useProject()`                                                                                            | The current project, kept up to date.                                                                                                            |
| `useSettings(pluginId)`                                                                                   | The plugin's settings in effect, and `save`.                                                                                                     |
| `useQuery`, `useQueryClient`                                                                              | TanStack Query, on Ahab's own client.                                                                                                            |

The module also re-exports everything in `@ahab/plugin/shared`
(`defineCommand`, `z` and the rest) and `definePlugin`. See the
[Reference](/docs/plugins/reference/) for every type.

```tsx
import { ConfirmDialog, useQuery } from "@ahab/plugin/web";

function Containers() {
	const list = useQuery({
		queryKey: ["docker", "containers"],
		queryFn: () => ahab.commands.call("docker.containers.list", {}),
		refetchInterval: 5_000,
	});
	// …
}
```

Prefix query keys with the plugin's ID: the client is shared with Ahab and
other plugins.

## Styling

Style a web entry with **Tailwind classes**, as Ahab's own components are.

- **Compiled at build.** `ahab-plugin build` scans `src/` for the classes
  it uses and writes them to `dist/web.css`, beside `dist/web.js`. Ahab
  loads that stylesheet with the entry. Arbitrary values
  (`grid-cols-[16rem_1fr]`) work as in any Tailwind project.
- **Ahab's theme.** The classes are compiled against Ahab's theme
  (`@ahab/plugin/theme.css`), so they follow Ahab's light and dark
  appearance. Use the colour roles rather than fixed colours:
  `background`, `foreground`, `heading`, `card`, `popover`, `primary`,
  `secondary`, `muted`, `accent`, `destructive`, `success`, `warning`
  (each with a `-foreground`), `border`, `input` and `ring`; for example
  `bg-muted text-muted-foreground`. Fonts: `font-sans`, `font-heading`,
  `font-mono`. Radii: `rounded-sm` to `rounded-xl`. `dark:` applies in
  Ahab's dark appearance.
- **Scoped to the plugin.** The classes apply only inside the plugin's own
  content, never to Ahab's pages or other plugins. Ahab wraps everything a
  plugin renders (views, windows, status bar items, its settings page,
  icons) in an element marked `data-ahab-plugin="<id>"`, and the stylesheet
  applies only under that mark. The kit's dialogs and menus carry the mark
  too, although they render elsewhere on the page. Content you move out of
  the plugin's boxes yourself (a React portal of your own) loses the
  plugin's classes.
- **Only utilities.** The stylesheet holds no reset or base styles; the page
  already has Ahab's. Ahab's own component classes, such as `pill`, are
  available as they are.

### Container queries

A plugin's view may be in a narrow sidebar, a wide tile or a phone's
screen. Size layouts by their container rather than the window: mark a box
`@container` and use `@md:`, `@lg:` and so on inside it.

```tsx
<div className="@container">
	<div className="grid gap-2 @md:grid-cols-[10rem_minmax(0,1fr)]">…</div>
</div>
```

## Phones

On a phone-sized screen (narrower than 768 pixels), the web app shows one
screen at a time:

- The plugin's views and activity icons are listed under a **More** tab
  beside Sessions and Files. A view opens there; an activity icon opens its
  window.
- Windows take the whole screen.
- There is no status bar: status bar items (`status.add`) are in the
  **Project status** sheet, opened from the top bar, each at least 44
  pixels tall.

Build views that work at phone width: container queries, buttons large
enough to tap, and no hover-only actions.

## Planned

Not built yet: items in Ahab's own tab, session and file menus; renderers
(a chat item kind, a file type); themes; keybindings; and moving Ahab's
own views and palette onto the same registries.
