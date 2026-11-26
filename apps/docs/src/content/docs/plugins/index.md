---
title: Plugin SDK
description: What an Ahab plugin is, what it can do today, and how far it is trusted.
---

A plugin adds to Ahab: views, windows and commands in the web app, and code
that runs on your machines.

:::caution[Experimental]
The plugin API is at version 0.1 and is experimental: it may change, and a
new minor version may break plugins without a deprecation period. See
[Versioning and compatibility](/docs/plugins/versioning/).
:::

## What a plugin is

A plugin is a folder, usually its own Git repository, with:

- **a manifest**, `ahab-plugin.json`: its ID, name, version, the plugin API
  versions it works with, and its entries. See the
  [manifest reference](/docs/plugins/manifest/).
- **a web entry** (`src/web/index.tsx`, built to `dist/web.js`): an ES
  module the web app loads in your browser. It adds views, windows, menu
  items, palette commands, status bar items and a settings page. See
  [Web entry](/docs/plugins/web-entry/).
- **a daemon entry** (`src/daemon/index.ts`, built to `dist/daemon.js`): a
  Node process that each machine's daemon runs. It answers commands, streams
  bytes over channels, publishes and hears events, and intercepts terminals
  before they start. See [Daemon entry](/docs/plugins/daemon-entry/).
- **shared definitions** (`src/shared/`): the commands, events, channels and
  settings both entries use, as [zod](https://zod.dev) schemas.

A plugin has only the entries it needs. Both are written in TypeScript with
the SDK, `@ahab/plugin`, and built with its `ahab-plugin build` command.

```text
my-plugin/
  ahab-plugin.json      the manifest
  package.json
  src/
    shared/index.ts     commands, events, channels
    shared/settings.ts  settings (optional)
    web/index.tsx       the web entry
    daemon/index.ts     the daemon entry
  dist/                 what the build writes, and what Ahab installs
```

## How a plugin runs

1. You install a plugin from a folder on the server's host or from a Git
   URL (`ahab plugins add`). The server copies its built files into its data
   folder. A new plugin starts disabled.
2. You enable it (`ahab plugins enable`, or Settings → Plugins).
3. Every browser with the web app open loads its web entry.
4. The server sends its daemon entry to every machine it runs on (all
   machines, or those you chose). Each machine's daemon starts it with
   Node 24 or later, restarts it after a crash, and stops it when the plugin
   is disabled.
5. A command called from the web app, the CLI (`ahab run`) or another
   plugin goes through the server to the right machine's daemon, which hands
   it to the plugin's process.

Entries never talk to each other directly: everything goes through Ahab.

## What works today

- Installing from a folder or a Git URL, enabling, disabling, updating and
  removing, on the command line and in Settings → Plugins.
- Development mode: `ahab plugins dev` rebuilds and reloads a plugin as
  you change it, without restarting Ahab.
- Web entries: sidebar views, activity bar icons, windows, **+** menu
  entries, palette commands, status bar items, a settings page, and Ahab's
  component kit and theme.
- Daemon entries: commands on a machine, channels, events, the
  `terminals.create` hook, and terminals the plugin starts.
- Settings with a schema, set per server, machine and project.
- Languages declared in the manifest: a Tree-sitter grammar and its
  queries for syntax highlighting, folding and the outline (experimental;
  see [`contributes`](/docs/plugins/manifest/#contributes)).
- Tools declared in the manifest: language servers, formatters, linters
  and checks, added to a language or replacing a default (experimental;
  see [`contributes.tools`](/docs/plugins/manifest/#contributestools)).
- Logs per plugin and machine (`ahab plugins logs`).

Planned, not built: server entries (a process next to the server, for
outside services), secrets, per-plugin storage, more hooks (chats, files,
Git), items in Ahab's own tab and session menus, progress reports for long
commands, a plugin registry, and sandboxing. Each page says what is planned
where it applies.

## Trust

Plugins are **not sandboxed**. Install only plugins you trust as much as
any program you run.

- A daemon entry runs as the user who runs Ahab's daemon, on **every machine
  it is enabled on**, with that user's access to files, credentials,
  processes and the network. Ahab starts it with a reduced set of
  environment variables, which is a convenience, not a boundary.
- A web entry runs inside the web app, in your browser, with your session:
  it can do anything you can do in Ahab.
- Ahab shows what a plugin declares (entries, hooks, where it runs) before
  you enable it, and records which plugins changed a terminal through a hook.

Sandboxing plugins is planned for later.
