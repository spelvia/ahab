---
title: Security
description: How Ahab lets clients in by pairing them, which connections it accepts, and how to get back in.
---

Anyone who can use Ahab can read and change files, open terminals and start
agents on every connected machine. So Ahab lets in only the **clients** you
paired with it: a browser, a phone, the command line on another computer, a
script. There are no accounts and no passwords. Each client has its own
credential, listed in **Settings → Clients**, where you can revoke it alone.

Machines are not clients: they join with their own keys (see
[Connect another machine](/docs/installation/cli/#connect-another-machine)). A
machine's daemon may call the API only for its own plugins, terminals and
projects, never another machine's (`403 other_machine_not_allowed`).

## How pairing works

A new client is always let in by something you already trust: a client that
is paired, or the server's own computer. There are two directions.

**A paired client shows a code.** Best for phones. On a paired client, choose
**Add client** (in Settings → Clients). It shows a QR code, the link it holds
and a code such as `K7QM-4RXT`. Scan the QR code with the new client, or open
the server's address there and type the code. They work once, for 10 minutes.
The dialog closes when the new client is paired.

**A new client asks; a paired client approves.** Best for the command line,
and for a browser you sit at. A browser that is not paired shows the pairing
screen with a short code such as `7F3K`. Every paired browser shows the
request (the client's name, its address and the code) with **Approve** and
**Deny**. On the command line:

```sh
ahab clients approve 7F3K    # or: ahab clients deny 7F3K
```

Approve only a code you can see on the new client. A request expires after
10 minutes.

The command line on another computer pairs the same way:

```sh
ahab login https://ahab.example         # asks, shows a code, waits for approval
ahab login https://ahab.example --code K7QM-4RXT   # with a code from Add client
ahab logout [https://ahab.example]      # revokes this command line and forgets it
```

`ahab login` opens the approval page in this computer's browser when it can,
so if that browser is paired, logging in the command line is one click. The
command line keeps its credentials per server in `logins.json` in its data
folder, readable only by you, never in `config.toml`.

For scripts and CI, **Create token** in Settings → Clients (or
`ahab clients token "<name>"`) makes a client of kind _token_ and shows its
credential once. Send it as `Authorization: Bearer <token>`, or set
`AHAB_TOKEN` for the `ahab` command.

Pairing links put their secret after `#` (`https://ahab.example/pair#…`), which
browsers never send to servers, so it stays out of proxy logs.

## First run

### On your own computer

`ahab up` starts the server, which writes the **local credential**:
`credential` in the data folder (`~/.local/share/ahab/credential` on Linux),
readable only by you, recorded as the client "This computer". The command line
on this computer uses it, so `ahab machines`, `ahab sessions` and the rest work
without logging in.

`ahab up` then opens your browser already paired. Another browser on this
computer shows the pairing screen; run this to open Ahab in it, paired:

```sh
ahab open --no-folder     # or `ahab open` in a folder, to open its project
```

### On a server without a screen

A homelab box, a VPS or a container. While no client but the server's own
computer is paired, every start prints a **first-run link** to its terminal
and its log (so `docker compose logs` and `ahab logs` show it):

```text
No client is paired yet. Open this link to pair your first one (once, within 24 hours):
  https://ahab.example/pair#9fK2…
```

It works once, for 24 hours; each start prints a new one, and earlier ones
stop working. It stops working as soon as any client is paired. Set
[`public_url`](/docs/configuration/#access) so the link uses the address you reach
the server by.

Instead, open the server in a browser, read the code on its pairing screen,
and approve it over SSH on the server with `ahab clients approve <code>`.

### Over SSH

From a computer that runs Ahab, `ahab login ssh://you@host` does all of it:
it installs and starts the server on that host, pairs this command line
(it asks, and the approval runs on the server's computer over SSH), and
prints a link that pairs a browser. `ahab machines add you@host` joins a
machine the same way. SSH is the proof: what you can do over SSH, you could
do at that computer's keyboard. Ahab uses your own `ssh` in batch mode and
never asks for or handles a password. See
[A server over SSH](/docs/installation/cli/#a-server-over-ssh).

## Connections

Credentials travel with every request, so Ahab accepts them only where nobody
along the way can read them, or where you would expect a home network:

- **HTTPS**, directly or behind a [trusted proxy](#behind-a-reverse-proxy) that
  says so;
- **this computer** (`localhost`);
- **plain HTTP on a private network**: a home or office network (`10.0.0.0/8`,
  `172.16.0.0/12`, `192.168.0.0/16`, `fc00::/7`, link-local addresses), or a
  tailnet (`100.64.0.0/10`, `fd7a:115c:a1e0::/48`, which WireGuard encrypts).
  Allowed; Settings → Clients marks clients that use it "Not encrypted".

Plain HTTP from a public address is refused with a page that says why
(`insecure_connection`). Set `allow_insecure_http = true` to allow it anyway.

Browsers allow some features only over HTTPS or on `localhost`, such as the
clipboard API; over plain HTTP on a network they are unavailable.

## Other guards

- **Host names:** the server answers only to `localhost`, IP addresses, the
  host of `public_url`, and the names in `allowed_hosts`; anything else gets
  `421 host_not_allowed`. This stops DNS rebinding and proxies serving Ahab
  under a name nobody listed.
- **Other websites:** requests that change something, and every stream and
  WebSocket, must come from the server's own origin, `public_url`'s, or one in
  `allowed_origins`. Other websites get `403 origin_not_allowed`.
- **Guessing:** pairing links and codes allow ten tries a minute per address,
  then a wait that doubles each time (`429 too_many_attempts`). After 50 wrong
  codes in an hour, every open code is withdrawn.

### Behind a reverse proxy

Forwarding headers (`X-Forwarded-For`, `X-Forwarded-Proto`,
`X-Forwarded-Host`, `Forwarded`) are believed only from `trusted_proxies`,
loopback by default. A proxy on the same computer works as it is; for one on
another host, list its address:

```toml
public_url = "https://ahab.example"
trusted_proxies = ["127.0.0.1", "::1", "10.0.0.2"]
```

With them, the client list shows the real address, rate limits count
it, and a proxy that terminates HTTPS makes the connection count as
encrypted.

## Clients

Settings → Clients lists every client: its name, kind, when and from where it
was last used, and who paired it; the one you use is marked "This client".
Rename or revoke any of them; the command line has the same:

```sh
ahab clients                      # list them (also `list`/`ls`)
ahab clients add                  # a pairing link, its QR code and a code
ahab clients rename <client> <name>
ahab clients revoke <client>      # asks first; --yes skips the question
ahab clients token <name>         # a token for a script, printed once
```

`ahab devices`, the former name of `ahab clients`, still works.

**Revoking** takes effect at once: the client's open streams and terminals
close, and its next request gets `401 not_paired`. Revoking "This computer"
writes a new local credential at once, so the server's own computer is never
locked out. Revoked clients are kept 30 days, then deleted.

Ahab never revokes a client by itself. A client unused for 90 days is marked
"Not used for 3 months", and a notice offers to review or revoke the unused
ones; dismissing it hides it until another client crosses the line.

## Recovery

- **Lost a phone or computer:** revoke its client from another one.
- **No paired client left:** on the server's own computer, run `ahab clients add`
  and open the link it prints, or approve a browser's code with
  `ahab clients approve <code>`.
- **Lost the server's computer:** that is losing the server; its data folder's
  backup holds everything. A new local credential is written at start.

## The command line inside Ahab's terminals

Terminals started by Ahab have `AHAB_SERVER` set to their server. On the
server's own computer, `ahab` there uses the local credential. On another
machine, the command line needs its own login once: run
`ahab login <server address>` there. Until then, client commands exit with
code `2` and say so.

## Limits

- Until sandboxing, agents run
  as you and can read the local credential, as they can read your SSH keys.
- Every paired client can do everything, including pairing others. Losing an
  unlocked phone means revoking it.
- A revoked browser keeps what it already loaded; Ahab cannot wipe its cache.
