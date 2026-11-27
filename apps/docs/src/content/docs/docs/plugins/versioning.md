---
title: Versioning and compatibility
description: The plugin API as a contract, the manifest's ahab range, and what Ahab promises while the API is experimental.
---

:::caution[The plugin API is experimental]
In Ahab 0.1 the plugin API is **experimental** and may change. Any Ahab
release may raise the plugin API to a new minor version that breaks
plugins, without a deprecation period: the manifest, the contexts, the
component kit, the protocol and the build can all change. Changes will be
listed with how to update.
:::

## The plugin API is a contract

The plugin API is everything a plugin relies on: the manifest's fields,
the web and daemon contexts, the `@ahab/plugin/web` kit, the theme's class
names, the hooks and their data, Ahab's events, and the protocol between a
daemon entry and Ahab. It has a version of its own, separate from Ahab's
release version. Today it is **0.1.3**: 0.1.1 added the manifest's
[`contributes`](/docs/plugins/manifest/#contributes) (languages), 0.1.2
added `contributes.tools` and languages' `tools` (language tooling), and
0.1.3 the daemon entry's [diagnostics API](/docs/plugins/daemon-entry/#problems)
and tool records' Checkstyle XML and report files.

Three things carry that version:

| Where                       | What                                                                    |
| --------------------------- | ----------------------------------------------------------------------- |
| Ahab                        | the plugin API version it offers (`0.1.3`)                              |
| The manifest's `ahab` field | the range of plugin API versions the plugin works with                  |
| `@ahab/plugin` (the SDK)    | the protocol version it speaks (`0.1.3`), sent in `initialize`'s answer |

## How the `ahab` range works

When a plugin is installed or updated, Ahab checks that its own plugin API
version is inside the manifest's `ahab` range, and refuses the plugin
otherwise, with a message such as "`>=1.0.0` does not include this Ahab's
plugin API, 0.1.3".

Write the range as npm does (`>=0.1.0 <0.2.0`) or as Rust does
(`>=0.1.0, <0.2.0`). See the
[manifest reference](/docs/plugins/manifest/#ahab) for the forms it accepts.

Ahab checks the range when the plugin is installed or updated. When a
daemon entry starts, it reports the protocol version its SDK speaks; Ahab
runs it only when that version is compatible with its own (the same minor
version before 1.0), and otherwise marks it failed with what to rebuild
against.

## What Ahab promises

The plugin API follows semver. Within one compatible version, changes are
additive only: new methods, new optional fields, new hooks and events. A
plugin keeps working until the version it names in its range is left
behind.

- **Before 1.0, as now:** the compatible version is the minor version.
  0.1.0 to 0.1.x are additive; 0.2.0 may break plugins. Hence the range to
  use: `>=0.1.0 <0.2.0`.
- **From 1.0:** the compatible version is the major version. A plugin for
  1.2 keeps working on 1.5; removing or changing something makes 2.0.

When to call the API 1.0 is not decided. Marking each extension point
stable or experimental in the SDK is planned.

## The SDK and Ahab change together

The SDK lives in Ahab's repository and changes with it. Build a plugin with
the SDK from the Ahab version it targets. Where and how `@ahab/plugin` is
published, and how its versions line up with Ahab's, is TBD.

The web entry uses Ahab's copy of `@ahab/plugin/web`, React and zod at run
time, not the ones it was built with. A plugin built against an older SDK
therefore gets the kit of the Ahab it runs in. The daemon entry bundles the
SDK it was built with.

## Planned

From the plugin distribution design, not built yet:

- a plugin registry: Ahab's own and community plugins, each release an
  archive with a checksum, listing the Ahab versions it supports;
- `ahab-plugin pack`, to write a release archive with its changelog;
- updates offered with their changelog, and a version that asks for more
  than the installed one left disabled until you approve it;
- signed releases for Ahab's own plugins.

The commands and names for these are TBD.
