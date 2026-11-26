---
title: Introduction
description: A minimal, self-hosted agentic workspace you can configure and extend with your own plugins.
---

Ahab is a minimal, self-hosted agentic workspace you can configure and extend
with your own plugins. **Build your own agentic workspace.**

Start with bundled tools for coding agents, files, terminals and Git. Keep
what you need, replace what you want to work differently, and build plugins
for the workflows that are yours. The workspace grows around how you work.

Pi Coding Agent and Obsidian are references for this approach: a small
foundation with extensions shaping the experience. In Ahab, that foundation
connects your machines, projects and tools through one web interface.

:::note[In development]
This page describes the product we are building. Today, the server, the CLI
and the web app work, with your own computer as the first machine: projects
(each one folder on one machine), agent sessions (Claude Code, Codex, Pi, and any
agent that speaks the Agent Client Protocol) with attachments and permission
modes, files, search, Git and terminals, and plugins: a first, experimental
version (see the [Plugin SDK](/docs/plugins/)). Dedicated review, sandboxing and
cross-agent orchestration are planned for later.
:::

## Build the workspace you need

Ahab keeps its core small: connected machines, projects, shared history,
commands and the foundation for plugins and views. Everyday tools such as
Agents, Terminal, Files and Git are bundled plugins, enabled by default.
You can disable or replace them.

Your own plugins use the same extension contracts as the bundled tools.
They can add views, commands and integrations. For example, you could build
a panel for a service you use, add a command for a project task, or create
a different view of an agent conversation.

Arrange those views in tabs and split panes, choose your settings and agent
profiles, and keep the tools that fit your workflow. Customization is part
of the product's foundation.

## Work with your agents

Start agent conversations inside your projects. Choose a model, attach files
or other context, follow responses and tool activity, and answer questions
and permission requests as they arrive.

Ahab works with the agent harnesses you already use. Each keeps its own
capabilities and configuration: model choices, planning controls, skills and
permission settings depend on what that agent supports.

Save reusable agent profiles with your preferred settings, instructions,
skills and MCP servers so you can start another session with the same setup.

## Keep your tools together

Arrange agent conversations, files, terminals and Git changes beside one
another in tabs and split panes. Read an agent's response, inspect the files
it changed, edit a document or run a test without leaving the workspace.

The bundled tools are designed to cover everyday work:

- **Agents:** conversations, streamed activity, questions, approvals and usage.
- **Terminal:** persistent shells and commands on your connected machines.
- **Files:** browsing, search and editing, including Markdown and an optional
  Vim keymap.
- **Git:** status, diffs, history, branches and explicit Git operations.

## Work across your machines

Connect a laptop, server or VM and see which agents are installed and ready
on each one. Choose where a session runs and which folder it works in.

Files stay on the machine that holds them. Ahab brings access to those
machines into one interface without copying or mounting their folders across
hosts. A session runs on one machine; linking another machine's folder into
a project does not automatically give that session access to it.

## Organize work into projects

A project is one folder on one machine: its path, and read-only or
read-write access for agents and tools. The folder can hold one repository,
several repositories or ordinary files. Agent sessions, terminals, files,
search and Git all work in the project's folder.

Give a project an optional **group** to list it under that heading in the
project switcher and the projects list; projects without a group come last.
Names need not be unique: where two projects share a name, the lists show
each one's machine and folder. The same folder can also be more than one
project, on one machine or on several.

The project switcher numbers the projects in the order it lists them:
Alt+1 to Alt+9 (⌥1 to ⌥9 on a Mac) switch to the first nine, and each row
shows its key. Drag a project by its handle, or press Alt+↑ and Alt+↓ on
it, to move it within its group; Settings → Projects keeps the same order.

Deleting a project removes it from Ahab; it does not delete the folder.

## Pick up where you left off

Session activity is recorded in a durable history. Reload the web app or
reconnect after a connection drops to recover the conversation and follow
new activity.

A new session is named after its first message. When the agent gives the
conversation a title of its own (ACP agents, Codex when its thread is
renamed, Claude Code when it writes one to its transcript), the chat and its
session take that title instead. A session or chat you rename keeps your
name.

Closing a browser tab or project view leaves its agents and terminals running. Closing an individual terminal tab inside a session asks for confirmation, then stops the process and removes its output.
Stopping work is an explicit action. Running processes still depend on the
machine that runs them staying online.

## Get started

Install the [CLI](/docs/installation/cli/): run Ahab on your computer and open
the web app in your browser. To develop Ahab, run it
[from source](/docs/installation/from-source/).
