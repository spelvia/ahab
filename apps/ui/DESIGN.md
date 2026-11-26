---
name: Ahab
description: The web app of a self-hosted agentic workspace; a quiet workbench in graphite and warm paper.
colors:
  sea-glass: "#8fb3a3"
  sea-glass-deep: "#315f57"
  amber: "#d9b26a"
  amber-deep: "#89590f"
  clay: "#e39a8a"
  clay-deep: "#a4412f"
  ink-on-glass: "#17201c"
  graphite: "#252526"
  graphite-raised: "#2b2d2e"
  graphite-muted: "#2d2d30"
  graphite-popover: "#353739"
  graphite-hover: "#3a3d3f"
  graphite-rule: "#3a3d40"
  graphite-field: "#45484b"
  ash: "#d4d4d4"
  ash-bright: "#f1f1f1"
  ash-muted: "#a8a8a8"
  warm-paper: "#f4f2ea"
  paper-raised: "#fbfaf6"
  paper-popover: "#ffffff"
  paper-muted: "#ebe8de"
  paper-hover: "#e6e3d8"
  paper-rule: "#e0dfd6"
  paper-field: "#d3d6ce"
  slate: "#27312e"
  slate-deep: "#182321"
  slate-muted: "#56635d"
typography:
  headline:
    fontFamily: "Manrope Variable, Manrope, system-ui, sans-serif"
    fontSize: "1.5rem"
    fontWeight: 800
    lineHeight: "2rem"
    letterSpacing: "-0.015em"
  title:
    fontFamily: "Manrope Variable, Manrope, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 700
    lineHeight: "1.5rem"
    letterSpacing: "-0.015em"
  body:
    fontFamily: "DM Sans Variable, DM Sans, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: "1.25rem"
  body-strong:
    fontFamily: "DM Sans Variable, DM Sans, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 600
    lineHeight: "1.25rem"
  label:
    fontFamily: "DM Sans Variable, DM Sans, system-ui, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 500
    lineHeight: "1rem"
  mono:
    fontFamily: "JetBrains Mono Variable, JetBrains Mono, ui-monospace, monospace"
    fontSize: "0.75rem"
    fontWeight: 400
    lineHeight: "1rem"
  mono-code-large:
    fontFamily: "JetBrains Mono Variable, JetBrains Mono, ui-monospace, monospace"
    fontSize: "2.25rem"
    fontWeight: 600
    lineHeight: "2.5rem"
    letterSpacing: "0.14em"
  mono-code:
    fontFamily: "JetBrains Mono Variable, JetBrains Mono, ui-monospace, monospace"
    fontSize: "1.5rem"
    fontWeight: 600
    lineHeight: "2rem"
    letterSpacing: "0.12em"
rounded:
  sm: "6px"
  md: "8px"
  lg: "10px"
  full: "9999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
  2xl: "32px"
components:
  button-primary:
    backgroundColor: "{colors.sea-glass}"
    textColor: "{colors.ink-on-glass}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    height: "36px"
    padding: "8px 16px"
  button-primary-sm:
    backgroundColor: "{colors.sea-glass}"
    textColor: "{colors.ink-on-glass}"
    rounded: "{rounded.md}"
    height: "32px"
    padding: "0 12px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.ash}"
    rounded: "{rounded.md}"
    height: "32px"
    padding: "0 12px"
  button-ghost-hover:
    backgroundColor: "{colors.graphite-hover}"
    textColor: "{colors.ash-bright}"
  button-destructive:
    backgroundColor: "{colors.clay}"
    textColor: "#ffffff"
    rounded: "{rounded.md}"
    height: "32px"
    padding: "0 12px"
  button-icon:
    backgroundColor: "transparent"
    textColor: "{colors.ash}"
    rounded: "{rounded.md}"
    size: "32px"
  nav-tab:
    textColor: "{colors.ash-muted}"
    typography: "{typography.body}"
    height: "48px"
    padding: "0 4px"
  nav-tab-active:
    textColor: "{colors.ash-bright}"
  settings-item:
    textColor: "{colors.ash-muted}"
    rounded: "{rounded.md}"
    padding: "6px 10px"
  settings-item-active:
    backgroundColor: "{colors.graphite-hover}"
    textColor: "{colors.ash-bright}"
  input-field:
    backgroundColor: "{colors.graphite}"
    textColor: "{colors.ash}"
    rounded: "{rounded.md}"
    height: "32px"
    padding: "0 8px"
  pill-muted:
    textColor: "{colors.ash-muted}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "2px 10px"
  waiting-count:
    backgroundColor: "{colors.amber}"
    textColor: "{colors.graphite}"
    rounded: "{rounded.full}"
    height: "16px"
    padding: "0 4px"
  table-row:
    textColor: "{colors.ash}"
    typography: "{typography.body}"
    padding: "12px 16px 12px 12px"
  waiting-band:
    textColor: "{colors.ash}"
    rounded: "{rounded.lg}"
    padding: "16px"
  inline-panel:
    backgroundColor: "{colors.graphite-raised}"
    rounded: "{rounded.lg}"
    padding: "16px"
  menu:
    backgroundColor: "{colors.graphite-popover}"
    textColor: "{colors.ash}"
    rounded: "{rounded.md}"
    padding: "4px"
    width: "176px"
---

# Design System: Ahab

## Overview

**Creative North Star: "The Quiet Workbench"**

Ahab's web app is a register you visit, not a dashboard you live in. Graphite layers by default (the theme follows `prefers-color-scheme`; dark unless the system asks for light), warm paper in light, hairline rules instead of boxes, and one cool sea-glass accent that marks where you are and what to do next. Everything else is neutral, so the single amber place on a page, something that waits for you, is impossible to miss.

Density is calm rather than cramped: 14px text, 12px row padding, one hairline between rows, no card chrome around page content. Structure comes from type weight, a fixed label column and rules, not from containers. Machine-readable facts (check codes, fingerprints, versions, commands, IDs) are always JetBrains Mono, so the eye can tell "what you read" from "what you compare or paste". Copy uses the product's own words (machine, project, this server) and controls name their action ("Add machine", "Approve", "Keep").

The onboarding design is the origin of this world; where it and the build differ, the build is recorded here. Notably, the shipped mark is the weave mark (`/ahab-mark.svg`, `/ahab-mark-white.svg` in dark), not the harpoon the onboarding brief still names.

**Key Characteristics:**

- Graphite dark default, warm-paper light; same roles in both.
- One accent (sea-glass), one alarm (amber), one danger (clay). Neutral everywhere else.
- Hairline rules and a 10rem label column carry structure; containers are reserved for things that interrupt.
- Manrope headings, DM Sans interface, JetBrains Mono for anything compared or copied.
- Flat at rest; motion is 150ms state change only.

## Colors

A near-neutral graphite / warm-paper ground with a single desaturated sea-glass accent, an amber that means "needs you", and a soft clay for destruction. Each role has a dark value (default) and a light value; components swap through the CSS custom properties in `src/shared/styles/globals.css`, never by hard-coding a hex.

### Primary

- **Sea-Glass** (dark `sea-glass`, light `sea-glass-deep`; `--primary`, also `--success` and `--ring`): the active top-bar tab underline, the primary button of a view (one per region: "Add machine", "Approve", "Save"), and the online status dot. It also drives system affordances: focus ring, text caret, and text selection (32% tint). Text on a sea-glass fill is `ink-on-glass` in dark and white in light.

### Secondary

- **Amber** (dark `amber`, light `amber-deep`; `--warning`, `--warning-foreground`): only for "waits for approval": the waiting band (12% fill, 50% border, 35% inner rules), its heading text, the pending status dot, and the count beside the Settings tab.

### Tertiary

- **Clay** (dark `clay`, light `clay-deep`; `--destructive`): error text, invalid-field borders, the destructive button and menu item, and the 6% tint of a row or panel that is asking to remove something. Never decorative.

### Neutral

- **Graphite / Warm Paper** (`graphite` / `warm-paper`; `--background`): the page and top bar (top bar at 90% with backdrop blur).
- **Raised** (`graphite-raised` / `paper-raised`; `--card`): only for an inline panel the user opened (Add machine).
- **Popover** (`graphite-popover` / `paper-popover`): menus.
- **Hover** (`graphite-hover` / `paper-hover`; `--accent`): menu-item focus, ghost-button hover, the selected settings item; 40% of it on table-row hover.
- **Muted** (`graphite-muted` / `paper-muted`): skeleton bars only.
- **Rule** (`graphite-rule` / `paper-rule`; `--border`): every hairline: top-bar bottom edge, table rows, section rules, panel borders.
- **Field** (`graphite-field` / `paper-field`; `--input`): input strokes and the scrollbar thumb.
- **Ash / Slate** (`ash` / `slate`; `--foreground`): body text. **Bright** (`ash-bright` / `slate-deep`; `--heading`): headings, names, active nav, the check code. **Muted** (`ash-muted` / `slate-muted`): descriptions, labels, table heads, inactive nav, "–" placeholders.

### Named Rules

**The Three Places Rule.** Sea-glass appears only on the active tab, the primary action and the online dot (plus the focus ring, caret and selection, which are the system's, not the page's). Links are not sea-glass: they are heading-coloured and underline on hover.

**The One Amber Place Rule.** Amber means exactly one thing: something waits for your approval. If nothing waits, no amber is on screen. A new surface that needs "attention" for another reason does not borrow amber without that meaning.

**The Swap, Don't Fork Rule.** Use the role (`primary`, `warning`, `border`...), never a theme-specific hex; both themes must work from one class list.

## Typography

**Display Font:** Manrope (variable, system-ui fallback), headings only, tracking -0.015em.
**Body Font:** DM Sans (variable, system-ui fallback), all interface text.
**Label/Mono Font:** JetBrains Mono (variable, ui-monospace fallback), for codes, fingerprints, IDs, versions, commands, and the rename field.

**Character:** A firm geometric heading face over a soft, readable interface face, with a mono that only appears where a value must be compared character by character or pasted into a terminal.

### Hierarchy

- **Headline** (Manrope 800, 1.5rem / 2rem): one per page (`h1`): "Machines", a machine's name, "Projects come here". A muted DM Sans 600 1rem suffix may follow inline for a live summary ("1 of 2 online").
- **Title** (Manrope 700, 1rem / 1.5rem): section and band headings (`h2`): "Overview", "Add a machine", "1 machine waits for approval".
- **Side-list label** (DM Sans 700, 0.875rem, heading colour): "Settings" above the side list; not a heading element.
- **Body** (DM Sans 400, 0.875rem / 1.25rem): everything readable; page descriptions are muted and capped at `max-w-3xl` with `text-pretty`. Names in rows are body at 600 in heading colour.
- **Label** (DM Sans 500, 0.75rem / 1rem): table headings, row meta ("Asked just now"), status messages, field hints. Sentence case, no tracking, no uppercase.
- **Mono** (JetBrains Mono 400, 0.75rem inline; 0.875rem in a command box): versions, fingerprints, IDs, inline commands like `ahab setup`.
- **Check code** (JetBrains Mono 600, 1.5rem with 0.12em tracking in the band; 2.25rem with 0.14em on the details page): the one large type in the app, sized to compare across two screens.

### Named Rules

**The Compare-or-Paste Rule.** Mono is for values a person compares or copies, and nothing else; never for labels, headings or decoration.

**The Tabular Rule.** Every `table` sets tabular numerals so times, versions and counts line up.

## Layout

- **Shell:** a sticky 48px top bar (mark + "Ahab" wordmark, then main tabs, 32px gap) over a single content column, both `max-w-6xl` (1152px) centred with 16px side padding; content has 32px top and bottom padding.
- **Settings:** a two-column grid, 12rem side list and the page, 40px gap. Below `md` (768px) the side list becomes one horizontal row of items above the page, ruled off by a hairline, with 24px gap.
- **Page rhythm:** list pages stack header, band, table at 24px; details pages stack at 32px. A page header is heading + one-line muted description on the left and the primary action top-right, wrapping under on narrow screens.
- **Tables:** full-width, 14px, 12px row padding, 16px cell gap, first cell 12px inset; columns drop by priority (Platform hides below `md`, Version below `sm` 640px), with the dropped value folded under the name as muted label text on mobile.
- **Details pages:** a breadcrumb ("Machines › name"), the headline with status dot and the ⋯ menu on the right, then titled sections. Each section is a definition list with top and bottom hairlines, rows divided by hairlines at 10px vertical padding, and a 10rem label column from `sm` up (stacked below).
- **Status and copy feedback** live in a fixed-height polite live region under the table or header, so messages never shift the layout.

### Named Rules

**The Rules-Not-Boxes Rule.** Page structure is drawn with hairline rules and whitespace. A bordered container appears only for something that interrupts the page: a waiting band, an opened inline panel, a removal confirmation, an empty state.

## Elevation & Depth

Flat. Depth comes from tonal steps (page, raised, popover) and hairlines, not shadows. The top bar separates from scrolled content with a hairline, 90% opacity and a backdrop blur.

### Shadow Vocabulary

- **Menu lift** (`box-shadow: 0 8px 24px -8px rgb(0 0 0 / 0.35)`): dropdown menus only, the one floating layer.
- **Status halo** (`box-shadow: 0 0 0 3px` in the dot's colour at 20%): online and pending dots, so a live state glows faintly; the offline dot is a hollow ring instead.

### Named Rules

**The Only-Floating-Things-Cast Rule.** Only an element that actually floats over the page (a menu) casts a shadow. Panels, bands and rows sit on the page and stay flat.

## Shapes

Gently rounded (8px, `--radius`) for every control: buttons, inputs, menus, settings items, the command box. Bordered containers that interrupt the page (waiting band, inline panel, removal panel, empty state) take 10px. Menu items take 6px inside their 8px menu. Pills, counts, status dots and the active-tab underline (2px) are fully round. Borders are always 1px hairlines; the empty state is the one dashed border.

## Components

### Buttons

Quiet and specific: each one names its action.

- **Shape:** gently rounded (8px); 36px default, 32px small (in rows, bands and forms), 24px extra-small for icon-only copy buttons.
- **Primary:** sea-glass fill, `ink-on-glass` text in dark / white in light, DM Sans 500 14px, 16px horizontal padding; hover drops to 90%. One per region.
- **Ghost:** no fill; hover takes the hover neutral (50% in dark). Used for the secondary choice next to a primary ("Reject", "Cancel", "Keep") and for icon buttons (⋯, close, copy).
- **Destructive:** clay fill (60% in dark) with white text, only inside a confirmation that names the thing ("Remove build-box? It must join again to come back.").
- **Outline:** bordered on the page background, for recovery actions ("Try again", "Back to home").
- **Focus / Disabled:** focus shows the sea-glass ring; pending actions disable at 50% opacity and re-label in progress ("Approving…").

### Chips

- **Pill (muted):** a hairline outline, muted label text, full radius ("This server"). The `pill-ok`, `pill-warn` and `pill-danger` tints exist in globals.css but are not yet used; when used they keep the colour meanings above (ok = live/online, warn = waits for approval, danger = failed).
- **Waiting count:** amber fill, page-colour 11px bold numeral, 16px tall, beside the Settings tab only while something waits.

### Cards / Containers

- **Corner Style:** 10px.
- **Background:** none for bands (amber tint) and empty states (dashed); `raised` for the inline panel the user opened.
- **Shadow Strategy:** none (see Elevation).
- **Border:** 1px hairline in the role's colour.
- **Internal Padding:** 16px; band and panel headers separated by an inner hairline.

### Inputs / Fields

- **Style:** 1px field-stroke, page background, 8px radius, 32px tall, 8px padding. The rename field is mono, since names are identifiers.
- **Focus:** 3px ring of sea-glass at 40%.
- **Error / Disabled:** clay border and a clay hint line below, in a reserved 16px row so it never shifts layout; Escape cancels an inline edit.
- **Command box:** mono command on the page background in a bordered 8px box with a ghost copy button at its right end.

### Navigation

- **Top tabs:** DM Sans 500 14px, 48px tall, 24px apart. Inactive muted, hover to body colour; active in heading colour with a 2px sea-glass underline along the bar's bottom edge.
- **Settings side list:** 14px 500 items with a 16px icon, 8px radius; active takes the hover neutral fill and heading colour; inactive muted, hover to 50% fill. On mobile it is a horizontal row of the same items.
- **Breadcrumb:** muted 14px, chevron separator, current item in body colour.

### Data Table (signature)

A dense hairline register: status dot, name (heading colour, 600, underline on hover, links to details), secondary columns muted, a ⋯ menu in the last 40px column. Row hover takes 40% of the hover neutral. Rename and remove happen in the row: rename swaps the name for a field with Save / Cancel; remove replaces the row with a clay-tinted confirmation. Focus returns to the name afterwards. Empty cells show "–". Loading shows three ruled skeleton rows, not a spinner.

### Status Dot

8px: sea-glass with halo when online, amber with halo when pending, a hollow muted ring when offline; always paired with a screen-reader word and a title.

### Row Menu

Behind a ghost ⋯ icon button, aligned to the trigger's end: popover tone, 8px radius, 4px padding, 176px minimum, menu lift. Items are 14px with a muted 16px icon; the destructive item sits last, after a hairline separator, in clay.

### Waiting Band (signature)

The one amber place: amber 12% fill, 50% border, 10px radius. A header row with the count as a title in amber and a muted instruction on the right; then one row per machine: pending dot, name, "Asked …", the check code in large mono, the fingerprint in small truncated mono, and Approve (primary, small) + Reject (ghost). On the details page the same band shows the code at 2.25rem.

### Inline Panel

Opening an action that needs setup (Add machine) inserts a raised, hairline-bordered panel under the page header instead of a modal; its trigger hides while it is open, and a ghost × closes it.

## Do's and Don'ts

### Do:

- **Do** keep sea-glass to the active tab, one primary action per region and the online dot (plus focus ring, caret, selection).
- **Do** reserve amber for "waits for approval" and remove it from the screen when nothing waits.
- **Do** structure pages with hairline rules, whitespace and a 10rem label column; put list content in a full-width table.
- **Do** set check codes, fingerprints, IDs, versions and commands in JetBrains Mono, and offer a copy button beside anything someone pastes.
- **Do** edit and confirm in place (inline rename, in-row removal, inline panel), returning focus where it came from.
- **Do** keep transitions to 150ms colour/state changes (hover, active tab, menu open).
- **Do** write controls as verbs naming their object, and use the product's words: machine, project, session, this server.

### Don't:

- **Don't** use cards as page structure or tile a page with boxed panels.
- **Don't** add stat tiles or number headers; a count belongs inline after the headline ("1 of 2 online").
- **Don't** put a kicker or eyebrow line above headings.
- **Don't** use gradient text or decorative colour; the only halo is the status dot's.
- **Don't** mark state with thick coloured side stripes; use the status dot or a full-border tinted band.
- **Don't** add entrance choreography, page transitions or spinners where a skeleton or a disabled, re-labelled button says it.
- **Don't** colour links with the accent; links are heading-coloured and underline on hover.
