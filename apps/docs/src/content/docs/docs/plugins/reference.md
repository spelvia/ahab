---
title: SDK reference
description: Every export of @ahab/plugin, by module, with its types.
---

`@ahab/plugin` has four exports and one command. This page lists all of
them; the other pages explain how to use them.

| Import                        | For                                                                   |
| ----------------------------- | --------------------------------------------------------------------- |
| `@ahab/plugin/shared`         | definitions both entries share: commands, events, channels, settings  |
| `@ahab/plugin/web`            | the web entry: `definePlugin`, the context's types, the component kit |
| `@ahab/plugin/node`           | the daemon entry: `definePlugin`, the runtime, the context's types    |
| `@ahab/plugin/theme.css`      | Ahab's theme for Tailwind, used by the build                          |
| `ahab-plugin build [--watch]` | builds the plugin in the current folder into `dist/`                  |

The SDK's source is TypeScript, run and bundled as it is; it needs Node 24
or later.

## `@ahab/plugin/shared`

Import it from `src/shared/`, and from either entry.

| Export                             | Kind     | What                                                                                 |
| ---------------------------------- | -------- | ------------------------------------------------------------------------------------ |
| `z`                                | value    | zod 4, the same copy Ahab uses.                                                      |
| `defineCommand(definition)`        | function | A command: `name`, `target`, `description?`, `input`, `output` (zod schemas).        |
| `defineEvent(definition)`          | function | An event: `name`, `data` (a zod schema), `lasting?`.                                 |
| `defineChannel(definition)`        | function | A channel: `name`, `params` (a zod schema).                                          |
| `defineSettings(shape)`            | function | The settings: a zod object from a shape of fields with defaults.                     |
| `jsonSchema(schema, io?)`          | function | A zod schema as JSON Schema, as input or output (the default).                       |
| `CommandDefinition<Input, Output>` | type     | What `defineCommand` takes and returns.                                              |
| `EventDefinition<Data>`            | type     | What `defineEvent` takes and returns.                                                |
| `ChannelDefinition<Params>`        | type     | What `defineChannel` takes and returns.                                              |
| `CommandTarget`                    | type     | `"machine"` or `"server"` (planned).                                                 |
| `Hooks`                            | type     | The hooks and what each carries: `"terminals.create"`: `TerminalCreate`.             |
| `TerminalCreate`                   | type     | `command?`, `env`, `folder`, `context`.                                              |
| `HookAnswer<Data>`                 | type     | `{ action: "allow" }`, `{ action: "change", data }` or `{ action: "deny", reason }`. |

Names (`containers.list`, `container.changed`, `logs`) are the plugin's
own; Ahab prefixes them with the plugin's ID.

## `@ahab/plugin/web`

Everything in `@ahab/plugin/shared`, and:

| Export                | Kind     | What                                                            |
| --------------------- | -------- | --------------------------------------------------------------- |
| `definePlugin(setup)` | function | The web entry: `setup(ahab: WebContext)`, may return a cleanup. |
| `WebContext`          | type     | The context: see below.                                         |
| `Target`              | type     | `{ machine?, project? }`: where a command or channel runs.      |
| `Ref`                 | type     | A command or channel: a name, or a definition (`{ name }`).     |
| `WebChannel`          | type     | `write`, `onData`, `onClose`, `close`.                          |
| `Project`             | type     | `id`, `machineId`, `name`, `path`.                              |

The component kit: `Button`, `IconButton`, `CopyButton`, `Dialog`,
`DialogTrigger`, `DialogClose`, `DialogContent`, `DialogTitle`,
`DialogDescription`, `DropdownMenu`, `DropdownMenuTrigger`,
`DropdownMenuContent`, `DropdownMenuItem`, `DropdownMenuSeparator`,
`ContextMenu`, `ContextMenuTrigger`, `ContextMenuContent`,
`ContextMenuItem`, `ConfirmDialog`, `NameDialog`, `Note`, `Markdown`,
`FileIcon`, `Terminal`, `toast`, `toastError`, `cn`, `useProject`,
`useSettings`, `useQuery`, `useQueryClient`. See
[the component kit](/docs/plugins/web-entry/#the-component-kit).

`WEB_EXPORTS` lists the names Ahab's web app must provide; Ahab's tests use
it.

### `WebContext`

| Member                                                             | Returns                          |
| ------------------------------------------------------------------ | -------------------------------- |
| `plugin`                                                           | the plugin's ID                  |
| `project()`                                                        | `Project` or `undefined`         |
| `commands.call(command, input?, target?)`                          | `Promise<Output>`                |
| `channels.open(channel, params?, target?)`                         | `WebChannel`                     |
| `events.on(name, listener)`                                        | a remover                        |
| `views.add({ id, title, icon, Component })`                        | a remover                        |
| `activity.add({ id, title, icon, opens: { window, params? } })`    | a remover                        |
| `windows.add({ kind, title, single?, Component })`                 | a remover                        |
| `windows.open(kind, params?)`                                      | nothing                          |
| `newWindow.add({ id, title, icon?, run })`                         | a remover                        |
| `palette.add({ id, title, keywords?, run })`                       | a remover                        |
| `status.add({ id, Component })`                                    | a remover                        |
| `settings.page({ Component })`                                     | a remover                        |
| `terminals.start({ project?, title?, command?, context?, show? })` | `Promise<{ session, terminal }>` |

## `@ahab/plugin/node`

Everything in `@ahab/plugin/shared`, and:

| Export                               | Kind     | What                                                                                                     |
| ------------------------------------ | -------- | -------------------------------------------------------------------------------------------------------- |
| `definePlugin(setup)`                | function | The daemon entry: starts speaking with Ahab over stdin and stdout, and sends the console to Ahab's logs. |
| `PluginError`                        | class    | `new PluginError(code, message)`: an error with a code.                                                  |
| `PROTOCOL_VERSION`                   | value    | The protocol version this SDK speaks: `"0.1.3"`.                                                         |
| `createRuntime(setup, io)`           | function | Runs `setup` over any pipe (`Io`); for tests.                                                            |
| `Setup`                              | type     | `(ahab: NodeContext) => void \| Remove \| Promise<void \| Remove>`.                                      |
| `NodeContext`                        | type     | The context: see below.                                                                                  |
| `CallContext`                        | type     | A command call: `project?`, `signal`, `progress`, `settings`.                                            |
| `Channel`                            | type     | `id`, `write`, `onData`, `onClose`, `close(reason?)`.                                                    |
| `TerminalInfo`                       | type     | `id`, `sessionId`, `projectId`, `command`, `state`, `exitCode`, `title`.                                 |
| `Io`                                 | type     | `write`, `read`, `exit`: the pipe `createRuntime` speaks over.                                           |
| `Diagnostic`                         | type     | A problem: `range`, `severity?`, `message`, `code?`, `fixes?` (0.1.3).                                   |
| `Fix`                                | type     | A quick fix: `title` and `edits` (`range`, `newText`) to the problem's file (0.1.3).                     |
| `DiagnosticCollection`               | type     | `name`, `set`, `delete`, `clear` (0.1.3).                                                                |
| `CheckRequest`                       | type     | What `onCheck` gets: `project` (`id`, `folder`), `roots`, `reason` (0.1.3).                              |
| `Position`, `Severity`, `ProjectRef` | type     | A place (`line`, `character?`), a severity word, a project by ID or as a check gives it (0.1.3).         |

### `NodeContext`

| Member                                 | Returns                                |
| -------------------------------------- | -------------------------------------- |
| `plugin`, `machine`, `ahabVersion`     | strings                                |
| `settings()`                           | the settings in effect on this machine |
| `onSettings(listener)`                 | a remover                              |
| `commands.handle(definition, handler)` | a remover                              |
| `commands.call(name, input, target?)`  | `Promise<unknown>`                     |
| `channels.handle(definition, handler)` | a remover                              |
| `hooks.before(hook, handler)`          | a remover                              |
| `terminals.create(options)`            | `Promise<{ session, terminal }>`       |
| `terminals.list()`                     | `Promise<TerminalInfo[]>`              |
| `terminals.kill(terminal)`             | `Promise<void>`                        |
| `events.publish(definition, data)`     | nothing                                |
| `events.on(name, listener)`            | a remover                              |
| `log.info`, `log.warn`, `log.error`    | nothing                                |
| `diagnostics.collection(name)`         | a `DiagnosticCollection` (0.1.3)       |
| `diagnostics.onCheck(handler)`         | a remover (0.1.3)                      |
| `onStop(listener)`                     | nothing                                |

## `ahab-plugin build`

Run in the plugin's folder (where `ahab-plugin.json` is).

| Source                   | Output                                               | Notes                                                                   |
| ------------------------ | ---------------------------------------------------- | ----------------------------------------------------------------------- |
| `src/web/index.tsx`      | the manifest's `entries.web`, and a `.css` beside it | ES module for browsers (ES2023), minified; shared modules left to Ahab. |
| `src/daemon/index.ts`    | the manifest's `entries.daemon`                      | ES module for Node 24, everything bundled.                              |
| `src/shared/settings.ts` | `dist/settings.json`                                 | Only when the file exists.                                              |

It also writes a `package.json` with `{ "type": "module" }` into each
output folder, and source maps. It fails when the manifest has no `id` or
no entries, names an unknown entry, names an entry as a binary per
platform, or when one entry imports another entry's files. A `server`
entry's source, `src/server/index.ts`, is built like a daemon entry, but
server entries do not run yet.

With `--watch`, it keeps building the entries on every change and reports
crossed imports without stopping, and writes `dist/settings.json` again
whenever a file in `src/shared` changes.
