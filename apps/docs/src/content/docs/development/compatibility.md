---
title: Compatibility
description: The rules Ahab and Stubb follow so that releases can change without breaking installs, machines, plugins or scripts.
---

Ahab runs as several parts that are not updated at the same moment: the
server, the daemons on other machines, the web app in an open tab, the
CLI in scripts, plugins written by others, and the data an install has
already stored. These rules keep each release compatible with what came
before. They apply to Ahab and to [Stubb](https://github.com/spelvia/stubb),
the library Ahab drives agents through.

:::note[Status]
These rules apply from the first release candidate, 0.1.0rc1. Some
checks they call for are being built; each section says what is in place
today.
:::

## What is versioned

One **product version** names a release of the `ahab` program: the server,
the daemon, the CLI and the web app it serves are built together and
always carry the same version. The **contracts** between Ahab and
everything outside one release are versioned on their own:

| Contract        | Version                                                      | Who depends on it                                                                            |
| --------------- | ------------------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| HTTP API        | the path, `/api/v1`                                          | the web app, the CLI, scripts, future desktop and mobile apps                                |
| Daemon protocol | an integer in the handshake, plus the methods a daemon lists | daemons of other releases                                                                    |
| Database schema | the migrations that have run                                 | every existing install                                                                       |
| Stored JSON     | a `<column>_format` number next to each JSON column          | every existing install                                                                       |
| Plugin API      | the `@ahab/plugin` SDK's version; a manifest's `ahab` range  | plugins (experimental in 0.1: see [Versioning and compatibility](/docs/plugins/versioning/)) |
| Files on disk   | the file's own format, kept unchanged                        | `config.toml`, `machine.key`, `server.key`, `outbox.jsonl`, the plugin store                 |
| CLI             | its commands, flags, `--json` output and exit codes          | scripts and agents                                                                           |
| Stubb           | its own semantic version                                     | Ahab and other hosts                                                                         |

## The rules

1. **Within a version, only add.** New fields are optional and have
   defaults. Nothing is renamed, removed or given another type or meaning.
   A change that cannot be made this way gets a new version: a new API
   path, a new daemon method, a new format number, a new migration.
2. **Readers are tolerant.** Unknown fields are ignored, never refused
   (`deny_unknown_fields` is not used for anything that crosses a version
   boundary). An enum read from outside has a catch-all for values it does
   not know.
3. **Writers write the current version; readers accept the current and
   the older ones.** Older data is upgraded when it is read, or by a
   migration.
4. **Unknown newer versions are handled, not misread.** A cache or a
   layout in a format the reader does not know is treated as absent; a
   chat event is kept as raw data and shown as unreadable; anything else
   is reported as invalid data rather than guessed at.
5. **Released migrations never change.** The database records a checksum
   of each migration it ran; editing one stops existing installs from
   starting. Every change is a new migration file, and every statement is
   safe to run twice (`IF NOT EXISTS`).
6. **New behaviour gets a new name.** A daemon method, an API route or a
   CLI command keeps its meaning; changed behaviour is a new one, and the
   old one stays until a major release.
7. **Removal follows deprecation.** Something to be removed is marked
   deprecated (in the docs, the API document and, where it applies, a
   warning) for at least one minor release first. Before 1.0, a minor
   release may break, but only with the change listed in the release notes
   together with how to adapt.
8. **Identifiers are forever.** IDs stored in data or settings (agent IDs
   such as `claude-code` and `acp:gemini`, feature IDs, shortcut IDs,
   plugin IDs, permission mode names, setting names) are never renamed;
   a new name is a new ID with a migration that maps the old one.

## Per contract

### Daemon protocol

The server works with every daemon release since 0.1: the oldest protocol
it accepts is only raised in a major release. A daemon lists the methods
it supports when it connects; the server never calls a method a daemon
lacks and reports `machine_outdated` instead, so the web app can ask for
that machine to be updated. A daemon from before this list existed is
treated as supporting protocol 1's methods.

### Database and stored JSON

Timestamps are whole microseconds since 1970 (UTC), IDs are lowercase
UUID text, and JSON is text with a `<column>_format` number written by its
store. Tests run every migration on a fresh database, run them again (a
no-op), and run each migration file twice.

### Stubb

Stubb follows semantic versioning. Public enums and the structs Stubb
builds for hosts are `#[non_exhaustive]`, so adding a variant or a field
is not a breaking change. The JSON Stubb's events serialize to carries a
format version, and unknown event types or fields deserialize to a
catch-all rather than failing, because hosts store events for a long
time. Releases are checked with `cargo-semver-checks`.

### The HTTP API and the CLI

Changes to `/api/v1` are additive; a breaking change is `/api/v2`, served
next to `/api/v1` for at least one minor release. The OpenAPI document
(`crates/ahab/openapi.json`) is generated and checked in, so every change
to the API shows in review. The CLI's `--json` output is the API's JSON
and follows the same rules; exit codes do not change meaning.

## Checks

| Check                                                                   | Status                                 |
| ----------------------------------------------------------------------- | -------------------------------------- |
| Migrations rerun safely, and each file runs twice                       | in place                               |
| Unknown stored JSON formats are handled                                 | in place                               |
| The OpenAPI document is current                                         | in place                               |
| Daemon capabilities and `machine_outdated`                              | in place                               |
| Recorded messages of each released daemon are replayed in tests         | in place (starts with 0.1's handshake) |
| Stubb: `#[non_exhaustive]`, event format version, `cargo-semver-checks` | in place                               |
| Recorded API answers and stored data of each release are read in tests  | planned                                |
