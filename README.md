<p align="center">
  <img src="apps/ui/public/favicon.svg" width="80" height="80" alt="">
</p>

<h1 align="center">Ahab</h1>

<p align="center">
  A self-hosted workspace for Claude Code, Codex, Pi, and any agent that speaks
  the Agent Client Protocol (ACP) over standard input and output.<br>
  Work with files, terminals, and Git alongside your agents, and extend the
  workspace with your own plugins.
</p>

## Installation

Ahab runs on Linux (x86_64 and arm64, glibc 2.36 or newer) and macOS
(Apple silicon). Install the `ahab` command with the install script. It
needs no root:

```sh
curl -fsSL https://github.com/spelvia/ahab/releases/latest/download/install.sh | sh
```

The script downloads the archive for your system, checks its SHA-256, puts
`ahab` in `~/.local/bin` and adds that folder to your `PATH` in your shell's
startup file (`~/.bashrc`, `~/.zshrc`, fish's `conf.d`, or `~/.profile`).
Open a new terminal, or run `export PATH="$HOME/.local/bin:$PATH"`, then
check it:

```sh
ahab --version
```

<details>
<summary>While the repository is private, and other options</summary>

Downloads from a private repository need a GitHub login, so download the
script and your system's archive with the [GitHub CLI](https://cli.github.com)
and install the archive you have:

```sh
gh release download --repo spelvia/ahab \
  --pattern install.sh --pattern 'ahab-linux-x86_64.tar.gz*'
sh install.sh ahab-linux-x86_64.tar.gz
```

Use `ahab-linux-aarch64` or `ahab-macos-aarch64` for other systems. The
script also reads these settings:

| Setting                    | What it does                                      |
| -------------------------- | ------------------------------------------------- |
| `AHAB_VERSION=v0.1.0-rc.1` | Installs that release instead of the latest one   |
| `AHAB_INSTALL_DIR=/path`   | Installs to that folder instead of `~/.local/bin` |
| `AHAB_NO_MODIFY_PATH=1`    | Leaves your shell's startup file alone            |

To build an archive from this checkout instead, run `make release` and
`sh scripts/install.sh target/dist/ahab-linux-x86_64.tar.gz`.

</details>

## Get started

Have your coding agent installed and signed in on this computer first:
Claude Code, Codex, Pi, or an ACP agent.

### Start Ahab

```sh
ahab up
```

This starts Ahab in the background, as a service that starts again when you
log in. It runs the **server**, which keeps Ahab's state and serves the web
app at <http://127.0.0.1:8180>, and the **daemon**, which makes this computer
a **machine** that agents run on. The first time, it registers this computer
under its hostname and opens the web app in your browser, already paired.

On a computer without a desktop, `ahab up` prints a one-time link instead.
Open it in your browser to pair it. To reach Ahab from another computer, see
[Configuration](apps/docs/src/content/docs/configuration.md) and
[Security](apps/docs/src/content/docs/security.md).

### Start your first agent session

1. Choose **New project**, pick your machine and a folder on it, and select
   **Create project**. The agent will work in that folder. From a terminal,
   `ahab open .` opens the current folder as a project.
2. In the project, choose **New session**, and select **Agent**. Pick
   your installed agent and send a prompt such as “What does this project do?”

### Day to day

| Command                      | What it does                                                       |
| ---------------------------- | ------------------------------------------------------------------ |
| `ahab status`                | What runs on this computer, and whether it answers                 |
| `ahab logs`                  | Ahab's logs                                                        |
| `ahab down`                  | Stops Ahab; it no longer starts at login                           |
| `ahab up`                    | Starts it again, and restarts it on a new version after you update |
| `ahab machines add you@host` | Runs agents on another computer too: installs Ahab there over SSH  |
| `ahab --help`                | Every command                                                      |

To update, run the install script again, then `ahab up`.

## Explore Ahab

- [User guide](apps/docs/src/content/docs/index.md) covers projects,
  connected machines, agent sessions, files, terminals, and Git.
- [Installation](apps/docs/src/content/docs/installation.md) describes the
  deployment options. [Configuration](apps/docs/src/content/docs/configuration.md)
  covers server settings, and [Security](apps/docs/src/content/docs/security.md)
  explains client pairing and access.
- [Plugin quick start](apps/docs/src/content/docs/plugins/quick-start.md)
  walks through building an extension for your workspace.

## Contributing

The server and CLI live in [crates/ahab](crates/ahab), the React app in
[apps/ui](apps/ui), and the Playwright tests in [e2e](e2e). Start with the
[source development guide](apps/docs/src/content/docs/installation/from-source.md).
Run `make` to list the available tasks. Use `make release` to build a local
archive for `scripts/install.sh`.

### Run Ahab from this checkout

To try Ahab from this checkout, you'll need rustup, Node.js 26 or newer,
Git, SSH, Make, and a native compiler and linker. Have your coding agent installed and ready to run on
this computer. For ACP agents, choose an entry from the ACP registry or add
a custom agent command in Ahab.

Run these commands from the repository root:

```sh
rustup show
npm install --global "$(node --print 'require("./package.json").packageManager')"
make setup
make dev
```

Open the [local web app](http://127.0.0.1:5183). If the browser shows a
pairing code, run `cargo run --bin ahab -- clients approve <code>` in a
second terminal from the repository root, replacing `<code>` with the code
on your screen. Ahab starts the server and connects this computer as your
first machine. Leave `make dev` running while you work.

<details>
<summary>Development setup, tests, ports, and CI</summary>

For `make test`, `make lint` and the Git hooks, install these tools too
(the hooks run them on each commit and push):

```sh
cargo install --locked cargo-nextest cargo-deny cargo-machete cargo-sort \
  taplo-cli typos-cli zizmor lychee
```

and [gitleaks](https://github.com/gitleaks/gitleaks#installing) (secrets).
knip (unused files, exports and dependencies) comes with `make setup`.

The desktop app (`apps/native/`) needs more: WebKitGTK 4.1,
libsoup 3, GTK 3, librsvg, libayatana-appindicator, OpenSSL, pkg-config and
the Tauri CLI (`cargo install tauri-cli`). Then `make app` builds its Linux
packages, `make app-test` and `make app-lint` check it, and `make app-dev`
runs it in development.

In development (`.env`) the server keeps its data in a SQLite file,
`data/ahab.db`, created on first start. To run it on Postgres instead, start
one with `make db` (Docker) and uncomment the Postgres `DATABASE_URL` in
`.env`.

The local API documentation is at
<http://127.0.0.1:8180/api/docs>.

### Tests

`make test` runs the server tests with cargo-nextest and the UI tests with
Vitest. Each server test gets a fresh SQLite database in a temporary folder.
`make test-postgres` runs the same server tests on a throwaway Postgres
(Docker, in memory), each on a new database dropped when the test ends, and
stops it afterwards; CI runs both. `make e2e` builds the server and the UI and runs
the Playwright tests against a fresh SQLite file. Install its Chromium browser
before running the end-to-end tests:

```sh
pnpm --filter @ahab/e2e exec playwright install chromium
```

On Linux, install the browser's system dependencies with
`pnpm --filter @ahab/e2e exec playwright install-deps chromium` if needed.
That step may require administrator access. Re-run the browser installation
when updating `@playwright/test` so the browser matches the package version.

### Ports

| Port  | What                                   |
| ----- | -------------------------------------- |
| 8180  | The server in development (`make dev`) |
| 5183  | The Vite dev server                    |
| 5184  | The documentation site (`make docs`)   |
| 8191  | The server in the end-to-end tests     |
| 55442 | Optional Postgres (`make db`)          |
| 55443 | Postgres for `make test-postgres`      |

They avoid the common defaults, so Ahab can run next to other local services.

### Continuous integration

`.github/workflows/ci.yml` checks the server and the UI on every pull request
and every push to `main`. Stubb comes from crates.io, so neither CI nor the
Docker builds need any credentials.

`.github/workflows/release.yml` builds and publishes a release when a
`v<version>` tag is pushed on a commit whose `main` build passed; see
[docs/releases](docs/releases/README.md).

</details>
