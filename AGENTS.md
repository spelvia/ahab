# Working on Ahab

Ahab is a minimal, self-hosted agentic workspace. Its main product promise
is that people can configure it and build or replace plugins to shape their
own workspace, with Pi Coding Agent and Obsidian as references for that
approach. This repository holds the server and command line (Rust), the web app
(React), the end-to-end tests and the documentation site. Agents are driven
through [Stubb](https://github.com/spelvia/stubb), a Rust library on
crates.io.

Its vocabulary: a **machine** is a computer that runs Ahab's daemon, a
**project** is one folder on one machine, a **session** is work with an
agent (its chats and terminals) in a project, and the **server** is the
central process that keeps Ahab's state and serves the API and the web app.
The documentation (`apps/docs/`) describes how Ahab behaves; when the code
and the documentation disagree, say so instead of quietly picking one.

## Layout

- `crates/ahab/`: the one Rust crate and its binary, `ahab`. `ahab up
  --foreground` runs the server and this computer's daemon;
  `ahab machines …` are client commands.
  - `src/cli/`: the entry point: the command tree (`command.rs`), starting Ahab
    on this computer (`up.rs`), and what commands share: the API client, exit
    codes, output and prompts.
  - `src/config/`: settings (flag, then environment, then `config.toml`, then
    default) and the config and data folders.
  - `src/server/`: the HTTP plumbing: router, errors, extractors, the
    services the routes reach (`state.rs`), the web app (`web.rs`), logging
    and startup (`start.rs`).
  - `src/daemon/`: the daemon's connection to its server and its request
    dispatch.
  - `src/modules/`: the features: `clients` (client, pairing: the browsers,
    apps, command lines and tokens paired with the server), `machines` (machine, identity, presence,
    enrollment, agents), `projects` (project, files, layout; it
    uses `machines`), `sessions` (chat, session; it uses `machines` and `projects`),
    `plugins` and `platform` (the database, and the core features on or off). Each module is a set of components; a
    component keeps its logic and the thin layers that expose it together,
    using only these file names: `model`, `service`, `store`, `api`, `http`,
    `cli`, `daemon`, `tests`.
  - Only the thin layers use plumbing: `http` uses the server's errors,
    extractors and state, `cli` the command line's client, exit codes, output
    and prompts. `model`, `service`, `store` and `api` use none of it, and
    `model` does no I/O.
  - Plain shared types go in `src/shared/`, outside systems in
    `src/integrations/`.
  - Modules are `foo/mod.rs`, never `foo.rs` next to `foo/`.
  - `tests/architecture.rs` checks the layout and the import rules. A new
    module is added to its `MODULES` table, after the modules it uses.
- `apps/`: what people run or read, one UI for every client.
- `apps/ui/`: the one UI, in the server's shape: the browser gets it from the
  server, and the desktop (later Android) app shows it too.
  - `src/modules/` has the same modules and components as the server, each
    component using the same file names: `index`, `api`, `stream`, `model`,
    `hooks`, `pages/`, `components/`.
  - `src/shared/` knows no module; `src/routes/` are thin and put modules
    together; `src/app/` wires it all.
  - Only a component's `api.ts` calls the generated client.
  - `scripts/check-layout.mjs` checks the layout and the import rules; its
    `uses` table mirrors the server's `MODULES`.
- `apps/native/`: the desktop (later Android) app's native side only: one
  Tauri project (its Rust shell, bridge and packaging). It has
  no screens of its own: it shows the web app, whose `modules/native` holds
  the app's first launch, servers and pairing. One UI for every client.
- `e2e/`: Playwright tests against the real server and the built UI.
- `scripts/install.sh`: installs a release archive (`make release` builds one
  for this computer).
- `apps/docs/`: the documentation site (Starlight). See [Documentation](#documentation).
- `docker/`: the image and the development services.

Stubb comes from crates.io, pinned to an exact version in `crates/ahab/Cargo.toml`.
To move Ahab to a newer Stubb, publish Stubb first and update the version. To
develop both at once, add a `[patch.crates-io]` entry pointing at the local
checkout to your own `~/.cargo/config.toml`, never to this repository.

## Commands

Install the host tools listed in `README.md`; the Rust toolchain is pinned
in `rust-toolchain.toml` and pnpm in `package.json`. Then:

| Command                | What it does                                                                    |
| ---------------------- | ------------------------------------------------------------------------------- |
| `make setup`           | Install dependencies and create `.env`                                          |
| `make dev`             | The server and the Vite dev server                                              |
| `make test`            | Server tests (nextest, on SQLite) and UI tests                                  |
| `make test-postgres`   | The server tests on a throwaway Postgres                                        |
| `make e2e`             | End-to-end tests in a browser                                                   |
| `make preview`         | Build the UI and serve it from a release server                                 |
| `make lint`            | Every linter and check                                                          |
| `make fmt`             | Format everything                                                               |
| `make openapi`         | Regenerate `crates/ahab/openapi.json` and the UI's API client after API changes |
| `make docs`            | Run the documentation site                                                      |
| `make grammars-update` | Rebuild the bundled Tree-sitter grammars and queries                            |
| `make tools-update`    | Pin each language tool's newest version and checksums in `tools/registry`       |

### Pairing a development browser

Every browser is paired with the server, the development one too.
The first time you open `make ui` (or a name in `AHAB_DEV_HOSTS`), it shows
the pairing screen with a short code; approve it from the repository with
`cargo run -- clients approve <code>`. The development `.env` points the
command line at the development server's `data/` folder, whose local
credential it uses. The development server also prints a first-run link while
nothing is paired. Tests never pair: they use a temporary data folder.

## Documentation

`apps/docs/` is Ahab's documentation site, built with Starlight (Astro): the user
guide and the technical documentation, in one place. It is the only home for
documentation; there is no separate knowledge base.

- Pages are Markdown or MDX in `apps/docs/src/content/docs/`; the sidebar is set in
  `apps/docs/astro.config.ts`.
- Every page is reviewed by a person before it lands. Add or restructure
  pages only when asked; propose them otherwise.
- The documentation also describes agreed features before implementation:
  this is the current development strategy. Clearly distinguish planned
  behaviour from what works today, and mark undecided commands and options
  as TBD. Do not invent installation commands, image names or chart names.
- When a change alters documented behaviour, update the page in the same
  change.

## Compatibility

Every release must keep working with what came before: daemons of other
releases, existing databases, plugins and scripts. The full standard is
`apps/docs/src/content/docs/development/compatibility.md`; in short:

- Within a version, only add: new fields optional with defaults; never
  rename, remove or retype a field, a method, a route, a command, a flag
  or a stored ID. Changed behaviour gets a new name.
- Readers tolerate the unknown: no `deny_unknown_fields` on anything that
  crosses a process or release boundary; enums read from outside have a
  catch-all.
- Released migrations never change; new ones are new files, idempotent.
- Stored JSON has a `<column>_format`; unknown newer formats are handled
  as the standard says, never misread.
- The daemon protocol stays backward compatible: the server only calls
  methods a daemon lists, and the oldest accepted protocol is raised only
  in a major release.

## Rules

- **Generated files are committed:** `crates/ahab/openapi.json`, the API client in
  `apps/ui/src/shared/api/generated/` (Orval, one folder per server component:
  request functions, TanStack Query hooks and MSW mocks) and
  `apps/ui/src/routeTree.gen.ts`. Regenerate them; never edit them by hand.
  So are the bundled languages: `apps/ui/languages/grammars/` (gzipped
  WebAssembly), `apps/ui/languages/queries/` (Helix's, MPL-2.0), the licence
  notices and `sources.json`'s `grammars`, all written by `make
  grammars-update` from the pins in `apps/ui/languages/sources.json`;
  `apps/ui/languages/languages.json` holds the declarations (file types,
  aliases), edited by hand except their paths. It needs git and the
  tree-sitter CLI (0.25 or later).
- **Database:** SQLite by default and Postgres optionally, through sqlx's
  `Any` driver; the backend comes from `DATABASE_URL`. Code that works with
  only one backend is a bug: keep repositories behind their traits, write
  plain SQL that both run (placeholders `$1, $2…`; no `jsonb` operators,
  arrays or `LISTEN/NOTIFY`), and add migrations as SQL files in
  `crates/ahab/migrations/`. Columns are what both share: UUIDs as `TEXT`,
  times as `BIGINT` microseconds since the Unix epoch in UTC, JSON as `TEXT`;
  stores convert with `shared/time.rs` and `shared/database.rs`, and select a
  `BOOLEAN` as `CAST(column AS INTEGER)`. Every JSON column has a
  `<column>_format INTEGER NOT NULL DEFAULT 1` beside it: the store writes its
  current format (a constant next to it) and never misreads a format it does
  not know (see `shared/database.rs`); change a JSON shape by raising the
  format, not in place. Every migration statement is `IF NOT EXISTS`, and a
  released migration never changes. `make test-postgres` and CI run every
  server test on both.
- **Server tests** get a fresh database each (`crates/ahab/tests/support/database.rs`)
  and never read `DATABASE_URL`, so they cannot touch the development database.
- **API:** errors use the Problem Details format with a stable `code`. Paths
  start with `/api/v1`; IDs are UUID v7; JSON fields and enum values are
  camelCase (`notSignedIn`, `readOnly`); error codes are snake_case
  (`machine_offline`). A fixed path segment never sits next to an ID
  segment that could take the same value (`/plugin-states`, not
  `/plugins/states` beside `/plugins/{id}`). Collections are added to with
  `POST` on the collection and removed from with `DELETE` on the item.
- **Access:** every new route is authenticated unless it is
  deliberately public. Routes under `/api/` need a paired client's
  credential by default; making one public means adding it to `PUBLIC` in
  `server/access.rs`, which `tests/server/access.rs` checks.
  Streams and WebSockets end when their client is revoked
  (`Caller::until_revoked`).
- **CLI:** every command works without a terminal, for scripts and agents:
  `--json` prints the API's JSON, exit codes follow `app/exit.rs`, and a
  question asked with `app/prompt.rs` (dialoguer) always has a flag that
  answers it instead.
- **Rust:** no `unwrap` or `expect` outside tests; `#[expect(lint, reason = "…")]`,
  never `#[allow]`; one `thiserror` type per layer; `tracing` with fields.
- **TypeScript:** no `any`, no default exports except config files, kebab-case
  file names, `@/` imports, server data only through TanStack Query.
- **Texts (i18n):** every text a person reads in the web app goes through
  i18next (`@/shared/i18n`, in `apps/ui/src/shared/i18n.ts`): `const { t } = useTranslation("<namespace>")`,
  then `t("key")`; outside React, `t` from `@/shared/i18n`. Each module has
  its namespace, `apps/ui/src/shared/i18n/locales/en/<module>.json`; the app
  shell's is `shell`, and `common` holds what the shared parts and everyone
  use (Cancel, Close, Loading…). Keys are nested by component or page and
  name what the text is for (`removeProject.title`), not its words; keys
  are typed, so a missing one fails `tsc`. Values interpolate with
  `{{name}}`; counts use i18next plurals (`key_one`, `key_other`, with
  `t("key", { count })`); a text with markup inside it uses `<Trans>`.
  Never build a sentence from pieces: one key per sentence. Numbers, dates
  and lists use `formatNumber`, `formatDate` and `formatList` from
  `@/shared/i18n`, not `toLocaleString`. English is the
  only language so far; another is a folder beside `en/` with the same files.
  Not translated: names (Ahab, Git, a machine's), code, paths, commands,
  log output, and texts the server sends.
- **Commits** follow Conventional Commits (`type(scope): summary`), as in
  Stubb; the `commit-msg` hook checks them.
- **Dependencies:** Dependabot updates them. Add new dependencies at their
  latest version; leave routine version bumps to it. `@playwright/test` is pinned in
  `e2e/package.json`; install matching browsers with
  `pnpm --filter @ahab/e2e exec playwright install chromium` after updating it.
- Never commit `.env` or secrets.
