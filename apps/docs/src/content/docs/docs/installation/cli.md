---
title: CLI
description: Install the CLI, run Ahab and open the web app with the recommended setup.
---

Installing the CLI on your host is the recommended way to install Ahab. The
one `ahab` executable runs Ahab on your computer (the server, and a daemon
that makes your computer a machine), joins other computers to a server, and
provides client commands. Once Ahab is running, use it through its web app in
your browser.

Desktop and mobile apps are optional and are not needed for this setup.

:::note[What works today]
Every command on this page works today. Releases are published on
[GitHub](https://github.com/spelvia/ahab/releases) for Linux (x86_64 and
arm64, glibc 2.36 or newer) and macOS (Apple silicon). While the repository
is private, downloading them needs a GitHub login.
:::

## Install

The install script puts the `ahab` binary in `~/.local/bin` (or
`AHAB_INSTALL_DIR`), checks the archive's SHA-256, adds the folder to your
`PATH` in your shell's startup file (unless `AHAB_NO_MODIFY_PATH=1`), needs
no root, and starts nothing:

```sh
curl -fsSL https://useahab.com/install.sh | sh                          # the latest release
curl -fsSL https://useahab.com/install.sh | AHAB_VERSION=v0.1.0-rc.1 sh # a given release
sh scripts/install.sh target/dist/ahab-linux-x86_64.tar.gz             # an archive from `make release`
```

The script comes from useahab.com; Ahab itself comes from the
[GitHub releases](https://github.com/spelvia/ahab/releases), checked against
each archive's SHA-256. `AHAB_RELEASE_URL` downloads the archives from
another address instead.

While the repository is private, download the script and the archive with
`gh release download --repo spelvia/ahab --pattern install.sh
--pattern 'ahab-linux-x86_64.tar.gz*'`, then run `sh install.sh
ahab-linux-x86_64.tar.gz`.

It prints the next step: `ahab up` to start Ahab, or `ahab setup` to join
another server.

Other computers need no install step of their own: from a computer that
runs Ahab, `ahab machines add you@host` installs it there over SSH (see
[Add a machine over SSH](#add-a-machine-over-ssh)), and `ahab login
ssh://you@host` installs a server there (see
[A server over SSH](#a-server-over-ssh)).

## Start Ahab

```sh
ahab up
```

This starts Ahab on your computer in the background and keeps it running:
it starts again when you log in. By default it runs both parts of Ahab:

- the **server**, which keeps Ahab's state, serves the API and the web app,
  and listens on `127.0.0.1:8180`;
- the **daemon**, which makes this computer a **machine**: the first time, it
  creates the machine's key and registers this computer under its hostname,
  then connects to the server.

`ahab up` waits until Ahab answers, prints its address, and opens the web app
the first time, in a browser that is already paired: this computer is listed
as an online machine, marked "This server". On a computer without a desktop,
it prints a one-time link to pair your first client instead (see
[First run](/docs/security/#first-run)). Run `ahab up` again after updating Ahab: it restarts Ahab on the new
version.

Database migrations run at every start. See [Configuration](/docs/configuration/)
for the settings, the config file and where Ahab keeps its data.

### Server only, or API only

```sh
ahab up --foreground --daemon=false   # the server alone, for example on a VPS
ahab up --foreground --web=false      # the API without the web app
```

Both settings are saved in `config.toml`, so the next start keeps them; give
`--daemon=true` or `--web=true` to turn them back on. The daemon does not run
as root, because agents would run as root too: run Ahab as your own user.

### Stop it, check it, read its logs

```sh
ahab down            # stop Ahab; it no longer starts at login
ahab down --remove   # and delete its service file
ahab status          # what runs here, whether it answers, this machine's state
ahab logs            # its last 100 lines; -f keeps following, -n sets how many
```

`ahab up` runs Ahab as a login service of your user, never as root: a
systemd user unit (`~/.config/systemd/user/ahab.service`) on Linux, a
LaunchAgent (`~/Library/LaunchAgents/dev.ahab.ahab.plist`, logging to
`~/Library/Logs/ahab/ahab.log`) on macOS. The service runs
`ahab up --foreground`, which you can also run yourself to keep Ahab in a
terminal (stop it with **Ctrl+C**); containers and development do that.

On a Linux computer you reach over SSH without a desktop session, a user
service stops when you log out unless lingering is on; `ahab up` says so and
gives the command (`loginctl enable-linger $USER`). Where no service manager
is available, as in most containers, `ahab up` runs in the terminal and says
so.

A service file that `ahab up` did not write, such as one from NixOS, Home
Manager or written by hand, means Ahab is managed by your system
configuration: `ahab up` and `ahab down` leave it alone and say what to run
instead, while `ahab status` and `ahab logs` still work.

## Open a folder

```sh
cd ~/code/my-app
ahab open                             # like `code .`: opens the web app at this folder
```

`ahab open [path]` finds the folder's project, or creates one named after
it, and opens the web app there (`--no-browser` prints the
address instead). The browser it opens is paired with the server; `ahab open
--no-folder` opens Ahab itself, for example in a browser that shows the
pairing screen. When the folder is several projects, `--project <project>`
says which. A project is **one folder on one machine**. Files never leave
their machine; Ahab reads them through that machine's daemon.

```sh
ahab projects                         # list them (also `list`/`ls`; --archived)
ahab projects show <project>          # its folder, machine, mode, group and state
ahab projects create <path> [--machine <machine>] [--name <name>] [--description "…"]
                     [--group <group>] [--read-only]
ahab projects edit <project> [--name <new-name>] [--description "…"] [--group <group>]
                     [--read-only | --read-write]
ahab projects archive <project>       # hide it, keep everything (restore brings it back)
ahab projects remove <project>        # asks first; files are never touched
```

`<project>` is a project's name or ID; when several projects share the name,
use the ID. `create` names the project after its folder unless `--name`
says otherwise, and uses the server's own machine unless `--machine` names
another; that machine must be online, so Ahab can check the folder. A blank
`--group` or `--description` removes it. The list shows projects under their
group, ungrouped ones last.

### Work with agents

In the web app, each project has its own **windows**, tiled and tabbed as
in VS Code and kept as you leave them: sessions, files, and new sessions.
Opening a window uses an italic **preview** tab, replacing the previous
preview in that pane. Double-click a tab, file, or session to keep it open;
a tab's right-click menu also offers **Keep open**. Plugin windows follow
the same rule. Typing in a window, moving it to another pane, or starting
a session keeps it open. Closing a preview does not add it to the list of
recently closed tabs.
A **New session** window (the `+` in any tab bar, or ⌘N) starts a
**session** with its first chat: choose one of the agents
that are ready on the project's machine (Claude Code, Codex or Pi), write
what it should do, and press Enter. The agent runs on that machine, in the
project's folder, and its work streams into the session's window. Use the
inner **+** to add another chat or terminal to the same session. Open as
many sessions side by side as you like; closing a window never stops its
agent. The **Permissions**
picker, next to the model, sets what it may do without asking, both before
you start and at any time during the chat:

- **Claude Code:** _Plan_ (reads and plans, changes nothing), _Ask_ (the
  default: asks in Ahab before editing files or running commands, and the
  Sessions view and the project switcher show that something needs you),
  _Accept edits_ (edits without asking, still asks before commands) or
  _Bypass_ (never asks).
- **Codex:** its mode is its sandbox and when it asks: _Read only_ (asks
  before going beyond reading), _Ask_ (the project's folder, without network
  access; asks before editing files and before any command that is not a
  plain read), _Workspace_ (the default: the same folder, asking only to go
  beyond it) or _Full access_ (never asks). A change applies from its next turn.
- **Pi** has no permission system; Ahab marks its sessions _unrestricted_.

A read-only project allows only Plan and Read only.

What an agent asks appears in the chat, where it happened, and waits for
you: a permission shows the command and its folder, the diff or the address
it is about, with **Allow**, **Allow for this session**, **Deny** (with an
optional reason) and **Deny and stop**; a question shows its options to
pick, plus a field for your own answer when the agent takes one; a plan is
shown in full, to approve (with the agent's options, such as accepting
edits from then on) or to send back with what to change; and a form (an MCP
server's request for details) shows its fields. With the request focused,
Enter allows and Escape denies. Answered requests stay in the chat, showing
what you chose. If you have scrolled up, the button back down turns into
**Needs your answer** and takes you to the request.

Ahab remembers each agent's model, reasoning effort and permission mode:
whatever you pick last, on the new-chat form or in a chat, is what the next
chat with that agent starts with, on every device.

To send files with a message, use the paperclip, drop them anywhere on the
chat, or paste an image. Each file (up to 10 MB, ten per message) is kept on
the project's machine in Ahab's data folder, outside the project. Images
go to the agent as images; every file's path is added to the message so the
agent can open it, without asking for permission.

Conversations are kept on the server, so you can read them from any device,
even while the machine is off. Agents stop when Ahab on that machine stops
(or restarts); writing to a stopped chat starts its agent again in the same
conversation, so it carries on where it left off.

From a finished turn in any chat, choose **Fork** to continue from that turn
in a new chat tab or a new session. The chat menu forks from the latest
finished turn. The fork opens the new-chat form with a **Chat history** chip:
the conversation up to that turn (your messages and the agent's, not its tool
calls; the oldest are left out past 250 KiB). Choose any agent and model,
including another provider's, and the history goes to it as context with
your first message. Remove the chip to start without it.

Each session's **⋯** menu (or a right-click on it in the Sessions view)
renames it, pins it to the top, moves it into a folder, archives it, or deletes it after confirmation.
Right-click empty space in the Sessions sidebar to create a session or folder.
Right-click a folder to add a nested folder or start a session there. Drag a
folder or session onto another folder to file it, or onto empty space to move
it to the top level. You can pin folders and sessions within their parent.
Archiving stops the session's agents (Ahab asks first if they are still
working) and moves it to **Archived** at the end of the list, where it can be
unarchived or deleted for good. Right after archiving, **Undo** brings it
back.

### Terminals

A terminal is a session too: **+ → New terminal**, the palette's _New
terminal_, or **⌃\`** (Alt+\` on Windows and Linux) opens your login
shell in the project's folder, on the project's machine. Any number of
windows, on any devices, show the same terminal and can type into it; it
takes the size of the window used last. Closing its inner terminal tab asks before stopping the process and deleting its output. Closing the project window leaves it running,
and it keeps running when Ahab restarts: a small `ahab terminal-host`
process on the machine keeps it. When its shell exits, the terminal stays
in the list until you archive it, with **Restart** in its menu (and
**Kill** while it runs). Archiving a running terminal ends it, after
asking. Its output is never stored by the server.

File paths in its output (`src/main.rs:42`) open the file at that line;
⌘/Ctrl+F searches what it printed; its tab shows the title its shell sets,
until you rename it. _Run a command…_ in the palette runs one command in a
terminal of its own, which stays to show the output. On a phone, a row of
keys (Esc, Tab, Ctrl, arrows) sits above the keyboard. Ahab's shortcuts
still work in a terminal: every ⌘ key on a Mac; elsewhere Alt with the
window keys, Ctrl+P and Ctrl+Shift+F, while Ctrl+K, Ctrl+W and the like
stay with the shell.

### Search and Git

**Search** (⌘⇧F) searches the project's files on its machine, following
`.gitignore`, with match case, whole word and regular expressions, and
files to include or exclude; a result opens the file at its line. ⌘P
opens a file by name.

**Git** works with the machine's own `git`, its configuration and
credentials. The Git view shows each repository with its branch and how
far it is behind and ahead (`↓0 ↑2`; clicking it syncs). Changes are one
list, as a list or a tree of folders: a checked file is staged, and Commit
commits the checked files (all of them when none is checked). The main
button follows the situation: **Commit** with changes, **Publish branch**
for a branch on no remote, **Sync changes** when behind or ahead; its ▾
has commit and push, commit and sync, and amend. The **Graph** shows the
commits of every branch with their branches and tags; a commit opens to
its files, each side by side. When the project's folder holds several
repositories, each is a section of its own. The branch is also in the
status bar; clicking it switches or creates branches. Ahab never waits for
a password: set up SSH keys or a credential helper on the machine for
remotes.

### Files and the editor

Files open read-only in the editor; **Edit** (⌘E) edits them, and selected
lines go to an agent's chat with **Add to chat** (⌘L). See
[Editor](/docs/editor/) for viewing, context for chats, saving and conflicts.
Its settings are in **Settings → Editor**, or from the command line:

```sh
ahab settings show editor [--project <project>]
ahab settings set editor <setting> <value> [--project <project>]
ahab settings unset editor <setting> [--project <project>]
```

### Keyboard

In a browser tab, browsers keep ⌘/Ctrl+T, W, N and Tab for their own tabs,
so Ahab's window keys use the modifier they leave free: **⌃ on a Mac, Alt
on Windows and Linux**. **T** opens a New session window, **W** closes the
tab in front, **Shift+T** reopens the tab closed last, **[** and **]** move
between tabs, and **\\** moves the tab into a tile on the right. ⌘K
(Ctrl+K) opens the palette, which lists every shortcut.

**Installed as an app**, Ahab gets the usual keys too: in its own window
the browser passes **⌘/Ctrl+T**, **⌘/Ctrl+W** and **Ctrl+Tab** (and
Ctrl+Page Up/Down) to Ahab instead of using them for its tabs. To install
it, open Ahab in a Chromium-based browser (Chrome, Edge, Vivaldi, Brave)
and choose _Install Ahab_ from the address bar or the browser's menu. Browsers
offer this only on HTTPS or `localhost`.

## Manage machines

```sh
ahab machines                         # list them (also `ahab machines list` or `ls`)
ahab machines show <machine>          # everything about one
ahab machines edit <machine> --name <new-name>
ahab machines remove <machine>        # asks first; --yes skips the question (also `rm`)
ahab machines add you@host [--name <name>]  # install, join and start it over SSH
ahab machines add [--name <name>]     # a one-time code for another computer
ahab machines approve <machine>       # approve a computer that waits to join
ahab machines agents <machine> [--refresh]  # Claude Code, Codex and Pi: ready?
```

`<machine>` is a machine's name, its ID, or a unique start of its ID. Names
use lowercase letters, digits and hyphens. The server's own machine cannot be
removed; turn its daemon off with `--daemon=false` instead. Removing another
machine means it must join again to come back. `ahab machine` works as well
as `ahab machines`.

`ahab machines agents` shows which coding agents a machine can run: whether
each is installed, its version, and whether it is signed in. Machines check
when they connect and every 15 minutes; `--refresh` checks now. Checking
only runs each agent's status command; it never starts a session.

Client commands talk to the server this computer runs, at the address in its
config. To use another server, pass `--server` or set `AHAB_SERVER`:

```sh
ahab login https://ahab.example.internal      # once: a paired client approves it
ahab machines list --server https://ahab.example.internal
```

On the server's own computer the command line needs no login: it uses the
server's local credential. Elsewhere, `ahab login` pairs it once; scripts can
set `AHAB_TOKEN` to a token instead. See [Security](/docs/security/) for
`ahab clients` and the rest.

Add `--json` to any client command to get the API's JSON instead of text.
Exit codes tell scripts and agents what happened: `0` done, `1` failed, `2`
used wrongly or not logged in (run `ahab login`), `3` rejected by the server, `4` not allowed, `5` try again
later (for example, the server cannot be reached).

## Sessions from the command line

```sh
ahab sessions [<project>] [--archived]    # most recent first; every project's by default (also `ls`)
ahab sessions show <session>              # its state, agent, model and mode
ahab sessions transcript <session>        # what you and the agent said (also `log`)
ahab sessions start <project> "<message>" [--agent claude-code|codex|pi]
                    [--model <model>] [--effort <effort>] [--mode <mode>] [--title <title>]
ahab sessions send <session> "<message>"  # an ended session continues
ahab sessions stop <session>              # stops the agent's current turn
ahab sessions rename <session> "<title>"
ahab sessions archive <session>           # and `restore`
ahab sessions remove <session>            # asks first; --yes skips the question (also `rm`)
```

`<session>` is a session's ID, or the short form `ahab sessions` shows (the
end of its ID). Sessions started here appear in the web app, and the other way
round, terminals included (their state is `running` or `exited`). `ahab
session` works as well as `ahab sessions`.

## Language tools

Language servers, linters, formatters and check commands run on each
machine (see [LSPs and Problems](/docs/language-tools/)). From the command line:

```sh
ahab lsp list [--machine <machine>] [--language <id>] [--rescan]
ahab lsp install <tool> [--machine <machine>] [--no-wait]
ahab lsp update <tool> [--machine <machine>] [--no-wait]
ahab lsp remove <tool> [--machine <machine>] [--no-wait]
ahab lsp status [--machine <machine>]
```

- `list` shows the machine's runtimes, then every tool with its language,
  kind, state (installed by Ahab, where it was found, available, not
  found, what it needs, off) and version, and whether an update is
  available. `--language rust` shows one language's tools; `--rescan`
  looks for runtimes and tools again.
- `install`, `update` and `remove` take a tool's ID as `list` shows it
  (`rust-analyzer`) and wait until the machine is done, then print the
  version installed. A failure says at which step, with the tool's last
  output. `--no-wait` returns once the job started.
- `status` lists the language servers that run: tool, state, memory and
  root.

`--machine` takes a machine's name or ID; without it, the server's own
machine.

```sh
ahab problems [--project <id>] [--severity <severity>] [--source <tool>] [--path <path>]
ahab check [--project <id>] [--tool <tool>]… [--repository <folder>]…
```

- `ahab problems` prints a project's problems as the server last saw
  them, one per line (`src/main.rs:3:9: error: … (E0308) [rust-analyzer]`),
  then the counts. `--severity warning` shows warnings and worse,
  `--source` one tool's, `--path` one file's or a folder's.
- `ahab check` runs the project's check commands now (`cargo clippy`,
  `tsc`), each at every root it has, and prints what each found. `--tool
  cargo-clippy` runs only that check, `--repository app/main` only within
  that repository or worktree; give either again for more. It exits
  `1` when errors remain or a check could not run, like a linter, so
  scripts and agents can use it.

The project is `--project`, else `AHAB_PROJECT` (set in Ahab's
terminals), else the project on the server's own machine whose folder
holds the current folder. `--json` prints the API's answer.

## Connect another machine

Join another computer where agents should work to your server, which can
run on this computer or on another host. There are three ways: over
SSH (recommended), with a one-time code, or with approval.

### Add a machine over SSH

From a computer whose `ahab` reaches the server (the server's own computer,
or one logged in with `ahab login`), one command installs Ahab on the other
computer, joins it and starts it:

```sh
ahab machines add you@build-box
ahab machines add ssh://you@build-box:2222 --name builder
```

It uses this computer's `ssh` and `scp`, so `~/.ssh/config`, your SSH agent,
keys and `ProxyJump` apply. SSH is the proof: the command creates a one-time
code itself, hands the other computer the server's key fingerprint to pin,
and checks that the machine the server approved has the key that sits on
that computer. Nobody types a code or compares a fingerprint. It never asks
for a password: SSH runs in batch mode, so a host that wants a password, a
passphrase or a new host key fails at once and says what to do (load the
key with `ssh-add`, or connect once with `ssh you@build-box` to accept its
host key).

Step by step, it says what it does:

1. looks at the other computer (`uname -sm`, an `ahab` it already has, its
   service manager);
2. copies this computer's `ahab` to `~/.local/bin/ahab` there, no root
   needed. Until releases are published, that works only when both run the
   same operating system and processor type; otherwise it stops and says
   so, and an `ahab` of the same version you put there yourself is used
   instead;
3. runs `ahab setup` there, which joins and starts the daemon as a login
   service, as `ahab up` does; on Linux it also turns on lingering
   (`loginctl enable-linger`) so the daemon keeps running after you log
   out, or says how when it may not;
4. waits until the machine is online.

Running it again on a computer that joined already repairs and updates it:
it copies a changed `ahab`, restarts it and checks it is online, and adds
nothing twice. Questions have flags: `--yes` when the computer runs a
server of its own or is a machine of another server. `--no-start` only
installs and joins. `--server-url` gives the address the other computer
reaches the server at, needed when the server answers only on this
computer (`127.0.0.1`); by default it is the server's `public_url`, else
the address it answers on. With `--json` it prints the result; exit codes
are as everywhere (`4` when SSH needs interaction, `5` when the host
cannot be reached).

In the web app, **Add machine → Over SSH** shows the same command.

### With a one-time code or approval

**With a one-time code**, approved at once. Create the code on the server with
**Add machine → With a one-time code** in the web app (Settings → Machines),
or with `ahab machines add`. Both show the command to run on the other
computer:

```sh
ahab setup --server https://ahab.example.internal --code ABCD-EFGH
```

A code works once, for 15 minutes.

**With approval.** Run `ahab setup --server https://ahab.example.internal`
without a code. It prints a link to the machine on the server (and a QR code
of it), a check code and the machine's key fingerprint, and waits. Open the
link, compare the check code with the one in the web app, and approve it
there, or with `ahab machines approve <machine>` on the server. A request
waits for 15 minutes at most.

Either way, `ahab setup` pins the server's key fingerprint: the daemon will
refuse a server with another key. Over plain `http`, it asks before trusting
the key (`--yes` answers for scripts). It saves the server in `config.toml`
and starts the daemon in the background, as `ahab up` does (`--no-start`
only joins). On that computer, `ahab up` runs only its daemon, connected to
the server, and client commands such as `ahab machines` talk to that server.
The machine opens the connection, so it needs no open ports.

To stop being a machine, run `ahab leave` on that computer: its daemon stops
and the server is forgotten in `config.toml` (it asks first; `--yes`
answers). The machine key stays, so joining again is quick; remove the
machine on the server (Settings → Machines) if it should go.

A machine's daemon may call the server's API for its plugins (starting and
listing their terminals, running plugin commands), but only for itself:
another machine's projects and terminals are refused
(`other_machine_not_allowed`), so a machine that is broken into cannot
reach the others through Ahab.

## A server over SSH

The same works for a server on another computer, a homelab box or a VPS:

```sh
ahab login ssh://you@homelab
ahab login ssh://you@vps --url https://ahab.example   # behind a reverse proxy
```

It installs Ahab on that computer as above, starts it there as a server
(with its own daemon, as `ahab up` does), and pairs this computer's command
line with it: the command line asks to pair, and the server's own computer
approves the request over SSH. Then it prints a one-time link that pairs a
browser (and opens it, on a desktop). Afterwards, reach the server with
`--server <its address>` or `AHAB_SERVER`.

The server's address is `--url`, saved there as `public_url`. Without it,
it is the address saved there before, else `http://<host>:8180`, the
host's name after `~/.ssh/config`. For a plain `http://` address the server
listens on every address of that computer, on that port; for `https://`
it keeps listening where it did, for a reverse proxy in front of it.
`--listen-addr` says otherwise. Plain HTTP is accepted only on private
networks and tailnets (see [Security](/docs/security/#connections)).

Running it again repairs and updates Ahab there and keeps this command
line's login. A computer that is a machine of another server is not turned
into a server. Where there is no service manager (most containers), it
stops before changing anything; run Ahab there yourself and use
`ahab login <address>`.

## Shell completions

`ahab completions <shell>` prints a completion script for bash, zsh, fish,
elvish or PowerShell. Load it the way your shell expects, for example:

```sh
# bash
ahab completions bash > ~/.local/share/bash-completion/completions/ahab
# zsh, with ~/.zfunc in your fpath
ahab completions zsh > ~/.zfunc/_ahab
# fish
ahab completions fish > ~/.config/fish/completions/ahab.fish
```

Generate the script again after updating Ahab, so new commands complete.

## Uninstall

```sh
ahab down --remove      # stop Ahab and delete its service file
rm ~/.local/bin/ahab    # the binary
```

To delete everything Ahab kept, remove its config and data folders as well
(see [Configuration](/docs/configuration/#files-and-folders)): on Linux
`~/.config/ahab` and `~/.local/share/ahab`, on macOS
`~/Library/Application Support/ahab`.
