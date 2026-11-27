---
title: Configuration
description: Configure Ahab's database, listening address, access, logging and CLI connection.
---

## Configuration model

Each setting comes from, in order: a command-line flag of `ahab up`, an
environment variable, the config file, then the default. A local `.env` file
is read into the environment in development; `make setup` creates one from
`.env.example`. Restart Ahab after changing settings.

`ahab up` saves no other flags for its background service: put settings that
should last in `config.toml` or the environment. `ahab up` passes
`AHAB_CONFIG_DIR` and `AHAB_DATA_DIR` on to the service when you set them.

## Files and folders

|                 | Linux                        | macOS                                            | Override          |
| --------------- | ---------------------------- | ------------------------------------------------ | ----------------- |
| Config file     | `~/.config/ahab/config.toml` | `~/Library/Application Support/ahab/config.toml` | `AHAB_CONFIG_DIR` |
| Data (database) | `~/.local/share/ahab/`       | `~/Library/Application Support/ahab/`            | `AHAB_DATA_DIR`   |

`$XDG_CONFIG_HOME` and `$XDG_DATA_HOME` are respected. The development `.env`
points both folders at the repository's `data/` folder.

`config.toml` holds the settings you saved; every key is optional:

```toml
daemon = true
web = true
listen_addr = "127.0.0.1:8180"
database_url = "postgres://ahab:ahab@127.0.0.1:55442/ahab"
```

On a computer that joined another server, `ahab setup` also writes that
server and its pinned key fingerprint. With them, `ahab up` runs only the
daemon:

```toml
server = "https://ahab.example.internal"
server_fingerprint = "SHA256:pzOEIYm+O2h2qr4zyWH5z7e6Ci1KtmtLD6JaNJ7Iei0"
```

The data folder holds the database (on a server), `machine.key` (this
computer's key as a machine), `server.key` (the server's key), `credential`
(the [local credential](/docs/security/#on-your-own-computer) of the server on this
computer) and `logins.json` (the command line's logins to other servers), all
readable only by you.

## Settings

| Flag of `ahab up`      | Environment variable | Config key     | Default                      | Purpose                                                                  |
| ---------------------- | -------------------- | -------------- | ---------------------------- | ------------------------------------------------------------------------ |
| `--daemon=true\|false` | `AHAB_DAEMON`        | `daemon`       | `true`                       | Also run this computer's daemon, so it is a machine too.                 |
| `--web=true\|false`    | `AHAB_WEB`           | `web`          | `true`                       | Serve the web app; the API is always served.                             |
| `--listen-addr`        | `AHAB_LISTEN_ADDR`   | `listen_addr`  | `127.0.0.1:8180`             | Address and port the server listens on.                                  |
| `--database-url`       | `DATABASE_URL`       | `database_url` | `ahab.db` in the data folder | SQLite file or Postgres connection URL.                                  |
| `--ui-dir`             | `AHAB_UI_DIR`        |                | unset                        | A built web app to serve instead of the one in the binary (development). |
| `--log-format`         | `AHAB_LOG_FORMAT`    |                | `text`                       | Log output format: `text` or `json`.                                     |

The settings that decide who may reach the server are in [Access](#access).

`--daemon` and `--web` given on the command line are saved to `config.toml`,
so they stay set for the next start. The other flags apply to one start only.

For example, to choose a different listening port when running from source:

```sh
cargo run --bin ahab -- up --foreground --listen-addr 127.0.0.1:8280
```

### Database

SQLite is the default, in the data folder. To use the repository's
development Postgres service, start it with Docker:

```sh
make db
```

Then uncomment the Postgres `DATABASE_URL` in `.env`:

```dotenv
DATABASE_URL=postgres://ahab:ahab@127.0.0.1:55442/ahab
```

Restart Ahab to connect to Postgres. Changing the URL selects a database; it
does not move existing data between databases.

### Access

Who may reach the server, and from where (see [Security](/docs/security/)). Each
list in an environment variable is separated by commas.

| Environment variable       | Config key            | Default  | Purpose                                                                                                                        |
| -------------------------- | --------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `AHAB_PUBLIC_URL`          | `public_url`          | unset    | The address people use, such as `https://ahab.example`. Its host is answered, and pairing links and the first-run link use it. |
| `AHAB_ALLOWED_HOSTS`       | `allowed_hosts`       | `[]`     | More names the server answers to, besides `localhost`, IP addresses and `public_url`'s host.                                   |
| `AHAB_ALLOWED_ORIGINS`     | `allowed_origins`     | `[]`     | More origins whose pages may call the API, such as a desktop app's.                                                            |
| `AHAB_TRUSTED_PROXIES`     | `trusted_proxies`     | loopback | Addresses or ranges (`10.0.0.0/8`) whose forwarding headers are believed.                                                      |
| `AHAB_ALLOW_INSECURE_HTTP` | `allow_insecure_http` | `false`  | Accept client credentials over plain HTTP from public addresses.                                                               |

For a server behind a reverse proxy at `https://ahab.example`:

```toml
public_url = "https://ahab.example"
trusted_proxies = ["127.0.0.1", "::1", "10.0.0.2"]
```

### Logging

Set `RUST_LOG` to choose the log filter and `AHAB_LOG_FORMAT` for the format.
The development `.env` contains:

```dotenv
RUST_LOG=info,ahab=debug
AHAB_LOG_FORMAT=text
```

Use `AHAB_LOG_FORMAT=json` for structured log output.

## CLI connection

Client commands talk to the server this computer runs, or the one it joined;
`--server` or `AHAB_SERVER` chooses another. `AHAB_LISTEN_ADDR` separately
controls where the server listens.

Every client command sends a credential: `AHAB_TOKEN` when set, else this
command line's login for that server (`ahab login`), else, for the server on
this computer, its local credential. Without one, a command exits with code
`2` and says to run `ahab login <server>`. See [Security](/docs/security/).

## Web app in development

`AHAB_DEV_PORT` sets the Vite development server's port; the default is `5183`.
The development server forwards `/api` requests to `AHAB_LISTEN_ADDR`.
The first time you open it, the browser shows the pairing screen: approve its
code from the repository with `cargo run -- clients approve <code>` (the
development `.env` points the command line at the development server's data
folder), or open the first-run link the development server printed.
Leave `AHAB_UI_DIR` unset: Vite serves the frontend in development, and
release builds carry the built web app inside the binary.

## Agent skills and MCP prompts

In a live chat, type `/` and select a skill or prompt. Add instructions and press Send. Results, questions and approvals appear in the same transcript as an ordinary turn. Available skills depend on the agent and project. Claude can also expose prompts from its connected MCP servers. Enter positional arguments in the composer for those prompts.

For named argument fields, a daemon operator can set `AHAB_MCP_PROMPT_CONFIG` to a trusted JSON file. Each outer key is the exact session working directory. Each server has an absolute executable path and an optional argument list. For example:

```json
{
	"/home/me/project": {
		"team": {
			"command": "/opt/team/bin/project-mcp",
			"args": ["--stdio"]
		}
	}
}
```

The daemon starts the configured server on demand in that directory. It lists the server's prompts and retrieves the selected prompt with the supplied arguments on the same connection. Ahab passes its ordered messages and content blocks as JSON in the user turn. The model receives text representations of embedded media. The configuration grants permission to launch the executable as the daemon user. Keep the file writable only by trusted operators. Reopen the session to pick up configuration changes.
