---
title: From source
description: Run Ahab from a source checkout.
---

These instructions run Ahab from a checkout: the server, the daemon that
makes your computer a machine, the web app and the CLI.

### Prerequisites

- Rust installed through rustup. The repository's `rust-toolchain.toml`
  selects the toolchain and includes rustfmt and Clippy.
- Node.js 26 or newer, plus the pnpm version pinned in `package.json`.
- Git, an SSH client and Make, with native compiler/linker tools for Rust
  on your operating system.
- A local checkout of the Ahab repository.
- SSH access to the private [Stubb repository](https://github.com/spelvia/stubb),
  which Cargo fetches as a pinned dependency.

Docker is optional; you only need it if you use the repository's Postgres
service instead of SQLite.

### Set up the repository

From the repository root, select the Rust toolchain and install dependencies:

```sh
rustup show
npm install --global "$(node --print 'require("./package.json").packageManager')"
make setup
```

`rustup show` installs or selects the pinned Rust toolchain. The npm command
installs the pnpm version declared by the repository. `make setup` installs
the JavaScript dependencies and creates `.env` from `.env.example` if it
does not exist.

### Start Ahab

From the repository root, run:

```sh
make dev
```

This starts Ahab (the server and this computer's daemon) and the web app's
development server. Open [the web app](http://127.0.0.1:5183): this computer
is listed as an online machine. The server listens on `127.0.0.1:8180`; the
web app forwards API requests to it.

The default database is SQLite. The development `.env` keeps it, the config
file and the machine key in the repository's `data/` folder: the server
creates `data/ahab.db` and applies migrations at startup. No separate
database service is required.

Press **Ctrl+C** to stop the development servers.

### Check the server

In a second terminal, list the machines:

```sh
cargo run --bin ahab -- machines list
```

It shows this computer, online. `curl http://127.0.0.1:8180/api/v1/health`
answers with the server's status and version.

The interactive API reference is available at
[`http://127.0.0.1:8180/api/docs`](http://127.0.0.1:8180/api/docs).

### Run the server on its own

```sh
cargo run --bin ahab -- up --foreground
```

To build the web app into a release binary and run it as installed, run
`make preview` instead of `make dev`, then open `http://127.0.0.1:8180`.

See [Configuration](/docs/configuration/) to change the database, listening
address or logging settings. Run `make` to list the development commands.

## Development checks

Install the extra Cargo tools used by `make test` and `make lint`:

```sh
cargo install --locked cargo-nextest cargo-deny cargo-machete
```

Before running `make e2e`, install Playwright's matching Chromium browser:

```sh
pnpm --filter @ahab/e2e exec playwright install chromium
```

On Linux, `pnpm --filter @ahab/e2e exec playwright install-deps chromium`
installs browser system dependencies when needed and may require administrator
access. Reinstall the browser after updating `@playwright/test`.
