---
title: Editor
description: Reading and editing files in Ahab, syntax highlighting and languages, adding lines, files, diffs, terminal output and search results to an agent's chat, and the editor's settings.
---

Files open in a code editor ([CodeMirror 6](https://codemirror.net/)) in
their own window: from the file tree, quick open (⌘P), search results, a
diff, or a path in a chat or terminal. Files open **read-only**; editing
is one deliberate step away.

:::note[Status]
Viewing, syntax highlighting, the outline, adding context to chats,
editing, conflicts and the settings below work today, and so do language
servers (diagnostics, hover, go to definition, completion,
formatting). See [LSPs and Problems](/docs/language-tools/).
:::

## Viewing

- **A file's tab** shows, on hover, where the file is on its machine and
  which machine (`/srv/app/src/main.rs — atlas`); its right-click menu has
  **Copy path** and **Copy relative path**. A session's tab shows its
  title, agent and machine; a terminal's its command and folder.
- **Find** with ⌘F (Ctrl+F), **go to line** with ⌃G (Ctrl+G).
- **Select** by dragging over text; drag in the line numbers to select
  whole lines, Shift-click to extend, ⌘-click (Ctrl-click) for several
  selections, ⌘A for all.
- **Folding** (the arrows beside the line numbers) follows the code's
  structure: functions, blocks, objects, Markdown sections. Languages
  without a grammar fold by indentation.
- **Expand the selection** to the code around it, a syntax node at a time,
  with ⇧⌥→ (Ctrl+Shift+⌘→ on macOS) or ⌘I; ⇧⌥← (Ctrl+Shift+⌘←) shrinks it
  back.
- **Outline** (the button above the text, or ⇧⌘O): the file's functions,
  classes, types and headings, nested; click one to go there, type to
  filter.
- Indentation guides.
- **Markdown** shows its source, its rendering, or both side by side; the
  source is the editor.
- Files over 2 MB show their first part and say so; they cannot be edited.
  Binary files are not shown.

## Highlighting and languages

Code is highlighted by [Tree-sitter](https://tree-sitter.github.io/), in
the browser: the same engine and colours in file windows, diffs, code
blocks in chats and Markdown, and search results. Nothing needs installing
on a machine.

- **Bundled languages:** Bash and other shells, C, C#, C++, CSS,
  Dockerfile, Go, HTML, Java, JavaScript (with JSX), JSON, Kotlin, Lua,
  Make, Markdown, Nix, PHP, Python, Ruby, Rust, SCSS, SQL, Swift, TOML,
  TSX, TypeScript and YAML. Each loads the first time a file needs it. Any
  other file shows as plain text.
- **Languages inside languages** are highlighted too: code in Markdown
  fences (by the fence's language: `rust`, `py`, `sh`…), CSS and JavaScript
  in HTML, front matter in Markdown.
- **A file's language** is found by its extension, then its name
  (`Dockerfile`, `Makefile`), then the program its `#!` line names, then an
  editor modeline (`vim: ft=python`). The language shows in the
  [status bar](#the-status-bar); click it to choose another for this
  window, or for every file of the same kind (`*.h`) in this browser, or
  plain text.
- **Large files** (over 2 MB, or with a line over 10,000 characters) show
  as plain text, and say so.
- **Colours** come from the colour theme's `--syntax-*` tokens (Ahab's own
  light and dark, or one chosen in Settings → Appearance).
- **Plugins** add languages, or replace a bundled one, by declaring a
  grammar and its queries in their manifest: see
  [`contributes`](/docs/plugins/manifest/#contributes).

The grammars and queries Ahab bundles, and their licences, are listed in
`THIRD_PARTY.md` in Ahab's repository; their licence texts are served at
`/licenses/languages.txt`.

## The status bar

The bar along the bottom of the workbench, as in VS Code. On the left, the
project's: its machine and connection (`● atlas`; offline or outdated
said in words; click for the machine's page) and the Git branch with
commits ahead and behind (click to switch branches); then the problem
counts `⊗ 2 ⚠ 1`, each only when there are some and a quiet check when
there are none (click to show or hide [Problems](/docs/language-tools/#problems);
right-click for the next and previous problem in the file); then plugins'
items. On the right, the focused file window's, and nothing when none is
focused:

| Item                       | What                                                                        | Click                                        |
| -------------------------- | --------------------------------------------------------------------------- | -------------------------------------------- |
| `Ln 10, Col 1`             | the cursor (columns count tabs to their stop), `(12 selected)` when some is | go to a line (and column)                    |
| `Spaces: 2`, `Tab Size: 4` | how the window indents                                                      | spaces or tabs and how wide, for this window |
| `UTF-8`                    | what the file was read as (`UTF-8 with BOM` when it starts with one)        |                                              |
| `LF`, `CRLF`               | how its lines end, found in the text (amber when mixed)                     | convert them (while editing)                 |
| `TypeScript`               | its language                                                                | choose another                               |
| `{} ✓`                     | its [language tools](/docs/language-tools/#in-the-file-window)              | what runs, Restart, Logs                     |
| `Read-only`, `Editing`     | the Edit switch                                                             | switch it (⌘E)                               |

A window's indentation is found as VS Code finds it: an `.editorconfig`
first, then what the text itself uses, then Settings → Editor's tab size;
what you choose in the status bar holds for that window until it closes
(**Find it again** goes back). Line endings are kept: a CRLF file is saved
with CRLF, and a file with both is saved with the more common one.

Ahab opens UTF-8 text. A file in another encoding shows as not text, as a
binary file does.

Phones have no status bar. The top bar shows the machine's connection as
a dot, with the number of problems beside it when there are some; tap it
for **Project status**, a sheet with every item of the bar as a row large
enough to tap: the machine, the branch, the problems, plugins' items and
the focused file window's. In a session, it is in the windows sheet.

The notification inbox is the bell in the top bar, on a phone too, where
it opens as a sheet the height of the screen.

## Problems in the editor

What the [language tools](/docs/language-tools/) report shows in the text, as
in VS Code, read-only too, and follows as you type:

- **Underlines**: errors red, warnings amber, information blue, hints
  dotted. A problem at a single point underlines the word there, or the
  line. Unused code is faded and deprecated code struck through.
- **The gutter** shows each line's worst problem; click it to see them.
- **The right edge** marks where problems are in the whole file; click a
  mark to go there.
- **Hover** an underline for the message, the tool and its rule (a link
  when the rule has a page), and the quick fixes offered, a language
  server's or those a plugin attached to its problem: click one to apply
  it while editing (⌘. lists them too). A fix changes the text like
  typing; saving it is a save like any other.
- **F8** and **⇧F8** go to the next and previous problem and show it.
- **Show problems at the end of the line** (Settings → Editor, off by
  default) writes each line's worst problem after it.

Besides what language servers and linters report live, a file window shows
the project's stored problems from tools that do not (check commands).

## The bottom panel

**⌘J** (Ctrl+J) shows or hides the panel under the windows, as in VS
Code; the problem counts in the status bar open it on Problems. It has two
tabs: [**Problems**](/docs/language-tools/#problems) (⇧⌘M) and
[**Output**](/docs/language-tools/#output) (⇧⌘U). Drag its top edge to resize
it, or maximise it. For each project, the browser remembers its height,
selected tab, and whether it is open or maximised. On a phone it takes the
whole screen.

## Adding to an agent's chat

Select lines and a small button appears by the selection: **Add to
"Fix flaky test"**. The chat named is the **target**: the session window
you used last in this project (typed in, brought forward or opened).

- Click it, or press **⌘L** (Ctrl+L), to add the lines to that session's
  composer.
- Its arrow, or **⇧⌘L** (Ctrl+Shift+L), opens the **chooser**: the
  sessions open here first, then recent ones, then **New session with
  this**, which opens a New session window with the lines already in its
  composer. The chat you choose becomes the target.
- With no session yet, ⌘L opens the chooser.
- Right-click a selection for **Add to chat** and **Copy lines**.

Several selections become several references, and partial lines count as
whole lines. A toast confirms what was added, with **Show** (brings the
session forward) and **Undo**, and the session's tab shows how many
pieces of context its composer holds.

The same works for:

| What            | Where                                                                                  |
| --------------- | -------------------------------------------------------------------------------------- |
| A whole file    | the file tree's right-click menu (or drag it into a composer), and the file tab's menu |
| A diff hunk     | the button above each hunk in a diff window                                            |
| Terminal output | select text in a terminal; the button appears at its bottom right                      |
| Search results  | the button beside the result count in the Search view                                  |

### In the composer

Context shows as **chips** above the message (`src/join.rs:40–58 ×`);
nothing is sent until you send. Remove a chip with its ×, click it to open
the file at those lines. Chips belong to the session, not the window:
they are kept in this browser until sent, also when the window closes.

### Opening files from a conversation

Expand a file change to open its source or select a line number. **Open diff**
shows the file's current Git changes in its checkout. For an edit without
line numbers, **Open change in editor** searches for the changed text.
If the text moved, Ahab can still find a unique match. If the text has several
matches or has disappeared, the action opens the file.

Expand consecutive reads to inspect each file. Search results with line
numbers and source locations such as `src/main.rs:12` open in the editor.
Command rows show the command, output, and any exit code the agent supplied.

A running task or subagent appears above the composer. Open it to follow the
log or conversation. **Stop** acts on the selected task or subagent when its
provider supports that action. The native receipt reports whether the provider
accepted the request. The item's status shows what happened afterwards.

### What the agent gets

Each reference goes after your message as text the agent can read: what
it is and where, and the text as you saw it.

````text
Why is this slow?

Context: src/join.rs, lines 40–58
```rust
…the lines as you saw them…
```

Context: README.md (the whole file)
````

A whole file is sent as its path; the agent reads it itself. When the file
changed on disk between selecting and sending, the line says
`(changed on disk since it was selected)`. In the conversation your
message shows with its references as chips, not the raw text.

## Editing

The **Edit** switch (the pencil above the text, or **⌘E**) makes the
window editable; it stays so until switched back or the window closes.
Read-only projects never offer it. Files can open editable instead:
see [Settings](#settings).

- **Save** with **⌘S** (Ctrl+S) or the Save button. Ahab saves only over
  the version the edit started from, atomically, keeping the file's
  permissions.
- **Unsaved changes** are marked on the tab and kept in this browser: a
  window closed and opened again brings them back (with **Discard them**).
- **Live updates:** open files follow changes on disk. Without unsaved
  changes the window shows the new content by itself.

### Conflicts

If the file changed on disk while you edited (an agent, Git or another
device), the window says so, and a save is refused rather than overwriting
the other change:

- **Compare** shows the file on disk and yours side by side.
- **Reload** drops your changes and shows the file as it is.
- **Save anyway** saves your version over it.
- **Keep mine** (before saving) hides the notice; saving still asks.

A file deleted on disk while open says so and offers **Save it again**.
Two devices editing one file work the same way: the second save is
refused; there is no live co-editing.

### Choosing and moving around

The file tree works as VS Code's Explorer does. A click chooses a file (and
opens it) or a folder (and opens or closes it); **⌘-click** (Ctrl-click)
adds or drops one, **Shift-click** chooses everything from the last one
chosen, in the order shown. With the tree focused:

| Keys                      | What                                             |
| ------------------------- | ------------------------------------------------ |
| ↑ ↓, Home, End            | Move; with Shift, choose as you go               |
| →                         | Open a folder, then go to its first file         |
| ←                         | Close a folder, or go to the folder a file is in |
| Letters                   | Go to the next name that starts with them        |
| Enter                     | Open the file (or open or close the folder)      |
| Space                     | Open the file as a preview                       |
| ⌘A (Ctrl+A)               | Choose everything shown                          |
| Escape                    | Choose nothing                                   |
| F2                        | Rename                                           |
| Delete, ⌘⌫                | Delete what is chosen                            |
| ⌘X, ⌘C, ⌘V (Ctrl+X, C, V) | Cut, copy, and paste into the focused folder     |

The right-click menu of a chosen row acts on everything chosen: add the
files to a chat, cut, copy, paste, copy their paths (one a line),
duplicate and delete (one question for all). A copy pasted into its own
folder, and **Duplicate**, are named `name copy.ext`; pasting asks about
names already there as dragging does, and its toast offers **Undo**.

The open file is revealed in the tree: its folders open and it is chosen.
Open folders stay open on your next visit (on this device); the button
above the tree closes them all.

### New, rename and delete

The file tree's right-click menus, and the buttons above it, make new
files and folders (in the focused folder, from the buttons; a path makes
its folders too), rename (a new path also moves it) and delete (a folder
with everything in it, after asking). Open windows follow a rename and
close on a delete. Read-only projects offer none of these.

### Moving by dragging

Drag files and folders in the file tree onto a folder to move them into
it, or onto the tree's empty space for the project's own folder; hold
**⌥** (Alt) to copy instead. Dragging a chosen row drags everything
chosen. A folder opens when you hover it while dragging, and the
tree scrolls near its edges. A folder never moves into itself. When the
folder already has something with the same name, Ahab asks: **Replace**
(cannot be undone), **Keep both** (the new one is named `name copy.ext`)
or **Cancel**. Open windows follow what moved and keep their unsaved
changes. The toast's **Undo**, or **⌘Z** in the tree soon after, moves it
back (or deletes the copy). Read-only projects take no drops. Copying
and replacing need the project's machine to run this version of Ahab;
moving works with older ones too.

## Settings

**Settings → Editor** holds the editor's settings for everyone on this
server; **Settings → Projects → Editor** sets a project's own, over them.

| Setting                              | Default | What                                                                        | Per project |
| ------------------------------------ | ------- | --------------------------------------------------------------------------- | ----------- |
| Files open editable                  | off     | Files open in Edit mode instead of read-only                                | yes         |
| Auto-save                            | off     | Save by itself: a moment after typing stops, or when the window loses focus |             |
| Auto-save delay                      | 1000 ms | For auto-save after a delay                                                 |             |
| Vim key bindings                     | off     | Vim keys; `:w` saves; see [Vim](#vim)                                       |             |
| Font size                            | 13      | In pixels                                                                   |             |
| Wrap long lines                      | off     |                                                                             |             |
| Line numbers                         | on      |                                                                             |             |
| Indentation guides                   | on      |                                                                             |             |
| Show problems at the end of the line | off     | Each line's worst problem after it                                          |             |
| Tab size                             | 4       | Columns per tab, and an indentation step                                    | yes         |

**Settings → Agent editor** groups what belongs to chats with agents. Its
composer settings are kept in the same `editor` section, for everyone on
this server:

| Setting                               | Default | What                                                             |
| ------------------------------------- | ------- | ---------------------------------------------------------------- |
| Messages sent while the agent works   | queue   | Queued until the agent's turn ends, or sent by stopping the turn |
| Vim key bindings in the chat composer | off     | Vim keys in the chat's message box; Enter sends from any mode    |

The rest of the page is kept on each device: whether thinking and tool
calls show expanded in the transcript, and whether a chat's **Session
usage** shows the tokens the session spent and its cost in US dollars, for
agents that report them. Beside the composer, Session usage is a ring that
fills with the agent's context; the numbers are in the popover it opens (a
sheet on phones).

Queued chat messages survive a server restart. If an agent session needs to
reopen, use **Send now** on a waiting message to continue from it. If the server
couldn’t tell whether the agent got a message, Ahab marks it as unconfirmed.
Retrying the same submission checks its receipt without sending it twice.
Check the conversation before sending it as a new message.

Auto-save goes through the same version check: a conflict stops it until
you choose. From the command line:

```sh
ahab settings show editor [--project <project>]
ahab settings set editor fontSize 14
ahab settings set editor editableByDefault true --project <project>
ahab settings unset editor editableByDefault --project <project>
```

## Keys

| Key        | What                                     |
| ---------- | ---------------------------------------- |
| ⌘F         | Find (and replace, while editing)        |
| ⌃G         | Go to line                               |
| ⌘L / ⇧⌘L   | Add to the target chat / choose the chat |
| ⌘E         | Edit, or stop editing                    |
| ⌘S         | Save                                     |
| ⇧⌘O        | Outline                                  |
| ⇧⌥→ / ⇧⌥←  | Expand / shrink the selection by syntax  |
| ⌘I         | Expand the selection by syntax           |
| F12 / ⇧F12 | Go to definition / references            |
| ⌘F12       | Go to implementation                     |
| F2 / ⌘.    | Rename / quick fixes (while editing)     |
| ⇧⌥F        | Format (while editing)                   |
| F8 / ⇧F8   | Next / previous problem in the file      |
| ⌘D         | Add the next occurrence to the selection |
| ⇧⌘K        | Delete the line                          |
| ⌥↑ / ⌥↓    | Move the line up / down                  |
| ⇧⌥↑ / ⇧⌥↓  | Duplicate the line above / below         |
| ⌘/         | Comment or uncomment the line            |
| ⌘J         | Show or hide the bottom panel            |
| ⇧⌘M / ⇧⌘U  | Problems / Output in the bottom panel    |

The panel's keys and the editor's own (find, go to line, save, edit, the
line commands, the language tools' keys, and Show the hover, which has
none by default) can be changed in **Settings → Keyboard**, under
**Editor**; like every shortcut there, the change is kept in this
browser. On Windows and Linux, Ctrl stands for ⌘. On macOS, expanding and
shrinking the selection is ⌃⇧⌘→ and ⌃⇧⌘←, as ⇧⌥ arrows select words there.

## Vim

With **Vim key bindings** on, the status bar shows Vim's mode
(`-- NORMAL --`, `-- INSERT --`, `-- VISUAL LINE --`…) and, while a
command is being typed, its keys (`d2`), as Vim's `showcmd` does. Vim's
own keys come first; the editor's keys above work where Vim leaves them.
`:w` saves. For the language tools:

| Keys        | What                                                                     |
| ----------- | ------------------------------------------------------------------------ |
| `gd`        | Go to definition                                                         |
| `gy`        | Go to type definition                                                    |
| `gi`        | Go to implementation; Vim's own `gi` where nothing finds implementations |
| `gr`        | Find references                                                          |
| `K`, `gh`   | Show the hover at the cursor (Escape closes it)                          |
| `]d` / `[d` | Next / previous problem                                                  |
| `gcc`, `gc` | Comment lines out, or back in                                            |

**Settings → Keyboard → Vim mappings** adds mappings, as Vim's `:map`
does: the keys typed, the keys they stand for, and the mode (normal,
insert, visual, or all three as `:map`), such as `jk` → `<Esc>` in insert
mode or `<C-s>` → `:w<CR>`. They are kept in this browser. A mapping is
non-recursive (`:noremap`) unless marked recursive, and only a recursive
one reaches the keys above (`gd`, `K`…). `<Leader>` is not supported.

## API

| Route                                               | What                                                                            |
| --------------------------------------------------- | ------------------------------------------------------------------------------- |
| `GET /api/v1/projects/{id}/file?path=&partial=`     | A file with its `version`; `partial` sends a large file's first part            |
| `PUT /api/v1/projects/{id}/file?path=`              | Save `{content, expectedVersion}`; `409 file_changed` when it moved on          |
| `POST /api/v1/projects/{id}/files`                  | A new file or folder `{path, kind, content}`; `409 already_exists`              |
| `POST /api/v1/projects/{id}/files/rename`           | Move `{from, to}`                                                               |
| `POST /api/v1/projects/{id}/files/move`             | Move or copy `{from, to, copy, replace}`; `409 already_exists` unless `replace` |
| `DELETE /api/v1/projects/{id}/file?path=`           | Delete a file (at `expectedVersion`, when given) or a folder                    |
| `GET /api/v1/projects/{id}/files/watch?path=&path=` | Server-sent events: `watching` with each file's version, then `changed`         |
| `POST /api/v1/chats/{id}/messages`                  | Gains `references`: `{kind, path, startLine, endLine, text, version, label}`    |
| `GET`/`PATCH /api/v1/settings/editor?project=`      | The editor's settings; `null` unsets a value                                    |

Writes answer `403 read_only` in read-only projects, and `409
machine_outdated` when the project's machine runs an Ahab from before the
editor: update that machine to edit its files (reading still works).
