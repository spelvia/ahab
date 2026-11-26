---
title: Settings, secrets and storage
description: How plugin settings are defined, scoped and delivered, and what is planned for secrets and storage.
---

## Settings

A plugin's settings are a zod object, defined once in
`src/shared/settings.ts` and exported as `settings`:

```ts
import { defineSettings, z } from "@ahab/plugin/shared";

export const settings = defineSettings({
	command: z
		.string()
		.default("docker")
		.describe("The Docker command line on each machine (or podman)."),
	logTailLines: z.number().int().min(10).max(5000).default(500),
});

export type Settings = z.infer<typeof settings>;
```

`ahab-plugin build` writes its JSON Schema (as input: fields with defaults
are optional) to `dist/settings.json`. Name that file in the manifest:

```json
"settings": "dist/settings.json"
```

Ahab reads the schema in when the plugin is installed or updated. Give
every field a default: a field without one, and not set anywhere, is absent
from the values the plugin gets.

### Scopes and precedence

Values can be set in three scopes, from the widest to the narrowest:

| Scope     | In Settings → Plugins | Applies to   |
| --------- | --------------------- | ------------ |
| `server`  | Everywhere            | everything   |
| `machine` | A machine             | that machine |
| `project` | A project             | that project |

The values in effect start from the schema's defaults; then each scope's
values are laid over them, widest first. A field set in a narrower scope
wins. Each field is replaced whole: an object set at a project replaces
the server's object, it is not merged with it.

Ahab checks values against the schema when they are saved, and refuses
fields the schema does not have and values that do not fit (a wrong type,
a value outside `enum`, a number out of bounds, a string of the wrong
length, in nested objects and lists too) with `validation_failed`, naming
each field, such as `values.pollSeconds`. Setting a field to `null`
unsets it in that scope. Values saved before a plugin's schema changed are
not checked again, so read them defensively.

### Where entries get them

| Where                               | Values                                                                    |
| ----------------------------------- | ------------------------------------------------------------------------- |
| Daemon entry, `ahab.settings()`     | the machine's: defaults, server, machine                                  |
| Daemon entry, `ahab.onSettings(fn)` | called with the machine's new values when any of those change             |
| A command's `call.settings`         | for its project when run for one (adds the project's), else the machine's |
| Web entry, `useSettings(id).values` | for the project open in the web app                                       |

`useSettings(id).save(values)` sets values in the server scope: the fields
given replace those there, the rest stay.

```tsx
import { useSettings } from "@ahab/plugin/web";

function Contexts() {
	const settings = useSettings("kubernetes");
	const values = (settings.values ?? {}) as Partial<Settings>;
	// …
	return <Button onClick={() => void settings.save({ contexts: next })}>Save</Button>;
}
```

### The generated form, or your own page

Settings → Plugins → the plugin shows a form generated from the schema,
with a scope picker. Each field shows what it inherits when not set in
that scope, and can be reset to inherit again. It handles:

- booleans, as a switch;
- fields with a fixed set of values (`z.enum`), as a list;
- numbers and text, as an input;
- arrays, as comma-separated text;
- objects, as JSON.

Field labels come from the field names (`logTailLines` is shown as "Log
tail lines"); a field's `.describe()` text is shown with it.

For settings a form cannot show well, add your own page from the web entry:

```ts
ahab.settings.page({ Component: SettingsPage });
```

With a page, Settings → Plugins shows it first, then the generated form
for the remaining simple fields under **Other settings**; fields that are
objects are left to your page. The page renders inside the plugin's scope,
so its Tailwind classes apply.

## Secrets

Planned, with server entries: secrets declared in the manifest's
`secrets`, entered by a person, stored encrypted by the server, and given
only to that plugin's server and daemon entries, never to the web app or
another plugin.

Today Ahab accepts `secrets` with a warning, shows the names before the
plugin is enabled, and does nothing else with them. Do not put secrets in
settings: settings are stored as they are and are visible to every web
entry.

## Storage

Planned: a key-value and document store per plugin on the server, scoped
like settings. It is not built; a daemon entry that needs to keep data
keeps it on its machine itself.
