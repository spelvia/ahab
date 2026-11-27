---
title: Installation
description: Install Ahab with its CLI, run it, and open the web app.
---

**Install the CLI.** It is one `ahab` executable: it runs the server and makes
your computer a machine, joins other computers, and provides client commands.
You use Ahab through the web app it serves; no desktop or mobile app is
needed.

| Method                                         | Choose it when…                                                      |
| ---------------------------------------------- | -------------------------------------------------------------------- |
| [CLI](/docs/installation/cli/)                 | You want to use Ahab: install it on your computer and run `ahab up`. |
| [From source](/docs/installation/from-source/) | You want to develop Ahab.                                            |

## Open the web app

Every Ahab server serves the web app. Open its address in your browser after
starting it: `http://127.0.0.1:8180` on your own computer, or the address
where you exposed a server elsewhere. Each browser is paired with the server
once; see [Security](/docs/security/).

`--web=false` turns the web app off if you use only the API or the command
line.

## Server and machines

The **server** holds Ahab's shared state and coordinates connected machines.
It serves the API and the web app.

A **machine** is a computer where agents work with local files and
processes, such as your laptop or a build box. Ahab's **daemon** runs on it
and connects outward to the server. Clients (browsers and the command line)
connect to the server, never directly to machines.

`ahab up` runs both on your computer by default: the server, and a daemon
that makes the server's own computer a machine. `--daemon=false` runs the
server alone. Other computers join with `ahab machines add you@host`, which
installs Ahab there over SSH, or with `ahab setup` on that computer.

## A server on another computer

To keep the server on a separate host, such as a home server or a VPS, run
`ahab login ssh://you@host`: it installs Ahab there over SSH and logs this
computer's command line in. Then join your working machines to it with
`ahab machines add you@host`, and open the server's web app to work with
them. Their files stay on their own machines.

See [Configuration](/docs/configuration/) for flags, environment variables and
settings.
