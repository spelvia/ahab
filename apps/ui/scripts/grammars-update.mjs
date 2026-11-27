#!/usr/bin/env node
// Rebuilds the bundled languages' assets from their pinned
// sources: `make grammars-update`.
//
// - Queries come from one upstream, Helix (MPL-2.0), at the commit pinned in
//   languages/sources.json. Helix's `; inherits:` lines are resolved, so each
//   file stands alone.
// - Grammars are the repositories and revisions that Helix commit names in
//   its languages.toml (so its queries match them), built to WebAssembly
//   with the tree-sitter CLI (`tree-sitter build --wasm`).
// - Each grammar's licence is read from its repository and must be MIT or
//   Apache-2.0; THIRD_PARTY.md and public/licenses/languages.txt are
//   rewritten from them.
//
//   node scripts/grammars-update.mjs                 rebuild at the pinned commit
//   node scripts/grammars-update.mjs --helix master  move to Helix's latest commit
//   node scripts/grammars-update.mjs --only rust,go  rebuild some languages only
//
// Needs git and the tree-sitter CLI (0.25 or later), which downloads its
// WebAssembly toolchain on first use.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const ui = fileURLToPath(new URL("..", import.meta.url));
const root = join(ui, "..", "..");
const dir = join(ui, "languages");
const work = join(root, "target", "grammars");
const sourcesFile = join(dir, "sources.json");
const languagesFile = join(dir, "languages.json");

/** The query files kept per language, when Helix has them. */
const QUERIES = ["highlights", "injections", "tags", "folds"];
const ALLOWED_LICENSES = new Set(["MIT", "Apache-2.0"]);

const args = process.argv.slice(2);
const option = (name) => {
	const at = args.indexOf(name);
	return at === -1 ? undefined : args[at + 1];
};

const run = (command, list, options = {}) =>
	execFileSync(command, list, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], ...options });

const sources = JSON.parse(readFileSync(sourcesFile, "utf8"));
const declarations = JSON.parse(readFileSync(languagesFile, "utf8"));
const only = option("--only")?.split(",");

// --- Helix, at the pinned commit (or a new one) -----------------------------

let commit = sources.helix.commit;
const wanted = option("--helix");
if (wanted) {
	const found = /^[0-9a-f]{40}$/.test(wanted)
		? wanted
		: run("git", ["ls-remote", sources.helix.repository, wanted]).split(/\s/)[0];
	if (!found) throw new Error(`Helix has no ${wanted}`);
	commit = found;
}
const helix = join(work, "helix");
fetchRepository(sources.helix.repository, commit, helix, ["runtime/queries", "languages.toml"]);
console.log(`Helix ${commit}`);

const grammarSources = readGrammars(readFileSync(join(helix, "languages.toml"), "utf8"));

// --- Queries ------------------------------------------------------------------

const queriesDir = join(helix, "runtime", "queries");
const header = (language, file) =>
	[
		`; From Helix (${sources.helix.repository}), commit ${commit},`,
		`; runtime/queries/${language}/${file}.scm. Licensed under the Mozilla Public`,
		"; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited",
		"; queries are written out in place of their `; inherits:` lines, and",
		"; the ones apps/ui/languages/sources.json adds (`inherit`) come first.",
		"",
	].join("\n");

/** A Helix query file with its `; inherits:` lines replaced by those files. */
function resolveQuery(language, file, seen = new Set()) {
	const path = join(queriesDir, language, `${file}.scm`);
	if (!existsSync(path) || seen.has(language)) return "";
	seen.add(language);
	return readFileSync(path, "utf8").replace(/^; inherits: *([\w.,-]+).*$/m, (_, list) =>
		list
			.split(",")
			.map((parent) => resolveQuery(parent.trim(), file, seen))
			.join("\n"),
	);
}

for (const [id, language] of Object.entries(sources.languages)) {
	if (only && !only.includes(id)) continue;
	const out = join(dir, "queries", id);
	rmSync(out, { recursive: true, force: true });
	mkdirSync(out, { recursive: true });
	const kept = [];
	for (const file of QUERIES) {
		// Ahab's own additions: Helix's TypeScript tags inherit no JavaScript
		// tags (functions, classes), though the grammars share those nodes.
		const extra = (language.inherit?.[file] ?? []).map((parent) => resolveQuery(parent, file));
		const text = [...extra, resolveQuery(language.queries, file)].join("\n");
		if (!text.trim()) continue;
		writeFileSync(join(out, `${file}.scm`), header(language.queries, file) + text.trimEnd() + "\n");
		kept.push(file);
	}
	const declaration = declarations.find((entry) => entry.id === id);
	if (!declaration) throw new Error(`languages.json declares no ${id}`);
	declaration.grammar = { ...declaration.grammar, wasm: `grammars/${language.grammar}.wasm.gz` };
	declaration.queries = Object.fromEntries(kept.map((file) => [file, `queries/${id}/${file}.scm`]));
	console.log(`queries ${id}: ${kept.join(", ")}`);
}

// --- Grammars -----------------------------------------------------------------

const treeSitter = run("tree-sitter", ["--version"]).trim();
const grammars = {};
for (const name of new Set(Object.values(sources.languages).map((language) => language.grammar))) {
	// An override replaces Helix's pick where it cannot be built (see sources.json).
	const override = sources.overrides?.[name];
	const source = override
		? { git: override.repository, rev: override.revision, subpath: override.subpath }
		: grammarSources.get(name);
	if (!source) throw new Error(`Helix ${commit} has no grammar ${name}`);
	const previous = sources.grammars[name];
	const used = Object.entries(sources.languages).filter(
		([, language]) => language.grammar === name,
	);
	if (only && !used.some(([id]) => only.includes(id)) && previous) {
		grammars[name] = previous;
		continue;
	}
	const checkout = join(work, "grammars", name);
	fetchRepository(source.git, source.rev, checkout);
	const license = licenseOf(checkout);
	if (!ALLOWED_LICENSES.has(license.spdx))
		throw new Error(`${name}: licence ${license.spdx} is not one Ahab bundles`);
	// Built to target/, kept gzipped: a twelfth of the size in the repository
	// and the binary; the engine unpacks it with DecompressionStream.
	const built = join(work, "wasm", `${name}.wasm`);
	mkdirSync(dirname(built), { recursive: true });
	run("tree-sitter", ["build", "--wasm", "-o", built, join(checkout, source.subpath ?? "")], {
		stdio: ["ignore", "inherit", "inherit"],
	});
	const bytes = readFileSync(built);
	const packed = gzipSync(bytes, { level: 9 });
	mkdirSync(join(dir, "grammars"), { recursive: true });
	writeFileSync(join(dir, "grammars", `${name}.wasm.gz`), packed);
	grammars[name] = {
		repository: source.git,
		revision: source.rev,
		...(source.subpath ? { subpath: source.subpath } : {}),
		...(override ? { override: true } : {}),
		license: license.spdx,
		bytes: bytes.length,
		gzipped: packed.length,
		sha256: createHash("sha256").update(bytes).digest("hex"),
		licenseText: license.text,
	};
	console.log(
		`grammar ${name}: ${(bytes.length / 1024).toFixed(0)} KiB (${(packed.length / 1024).toFixed(0)} KiB gzipped), ${license.spdx}`,
	);
}
// Grammars no language uses any more.
for (const file of readdirSync(join(dir, "grammars"))) {
	if (!(file.replace(/\.wasm\.gz$/, "") in grammars)) rmSync(join(dir, "grammars", file));
}

// --- The pin, the notices -----------------------------------------------------

const texts = Object.fromEntries(
	Object.entries(grammars).map(([name, grammar]) => [name, grammar.licenseText]),
);
for (const grammar of Object.values(grammars)) delete grammar.licenseText;
const previousTexts = existsSync(join(ui, "public", "licenses", "languages.txt"))
	? readLicenses(readFileSync(join(ui, "public", "licenses", "languages.txt"), "utf8"))
	: {};

sources.helix.commit = commit;
sources.treeSitter = treeSitter;
sources.grammars = Object.fromEntries(Object.entries(grammars).toSorted());
writeFileSync(sourcesFile, JSON.stringify(sources, null, "\t") + "\n");
writeFileSync(languagesFile, JSON.stringify(declarations, null, "\t") + "\n");

writeNotices();
run("pnpm", ["exec", "oxfmt", sourcesFile, languagesFile, join(root, "THIRD_PARTY.md")], {
	cwd: root,
});
console.log("done: review git diff, then run the syntax tests (pnpm --filter @ahab/ui test)");

// ---------------------------------------------------------------------------

/** A shallow checkout of `repository` at `revision`, only `paths` when given. */
function fetchRepository(repository, revision, into, paths) {
	const current = existsSync(join(into, ".git"))
		? run("git", ["-C", into, "rev-parse", "HEAD"]).trim()
		: "";
	if (current === revision) return;
	rmSync(into, { recursive: true, force: true });
	mkdirSync(into, { recursive: true });
	run("git", ["-C", into, "init", "--quiet"]);
	if (paths) {
		run("git", ["-C", into, "sparse-checkout", "set", "--no-cone", ...paths]);
	}
	run("git", [
		"-C",
		into,
		"fetch",
		"--quiet",
		"--depth",
		"1",
		"--filter=blob:none",
		repository,
		revision,
	]);
	run("git", ["-C", into, "checkout", "--quiet", "FETCH_HEAD"]);
}

/** `[[grammar]]` entries of Helix's languages.toml: name, git, rev, subpath. */
function readGrammars(toml) {
	const found = new Map();
	for (const block of toml.split(/^\[\[grammar\]\]\s*$/m).slice(1)) {
		const name = /^name\s*=\s*"([^"]+)"/m.exec(block)?.[1];
		const field = (key) => new RegExp(`${key}\\s*=\\s*"([^"]+)"`).exec(block)?.[1];
		if (name && field("git"))
			found.set(name, { git: field("git"), rev: field("rev"), subpath: field("subpath") });
	}
	return found;
}

/** The repository's licence: its SPDX name and text. */
function licenseOf(checkout) {
	const file = readdirSync(checkout).find((name) =>
		/^(LICEN[CS]E|COPYING)(\.(md|txt))?$/i.test(name),
	);
	if (!file) return { spdx: "none", text: "" };
	const text = readFileSync(join(checkout, file), "utf8");
	const spdx = /Permission is hereby granted, free of charge/.test(text)
		? "MIT"
		: /Apache License[\s\S]*Version 2\.0/.test(text)
			? "Apache-2.0"
			: "unknown";
	return { spdx, text };
}

/** The licence texts in public/licenses/languages.txt, by grammar. */
function readLicenses(text) {
	const found = {};
	for (const part of text.split(/^={10,}\n/m)) {
		const name = /^tree-sitter grammar: (\S+)/.exec(part)?.[1];
		if (name) found[name] = part.split("\n").slice(3).join("\n").trim();
	}
	return found;
}

function writeNotices() {
	const rule = "=".repeat(78);
	const parts = [
		"Third-party notices for Ahab's bundled languages.",
		"",
		"The grammars below are compiled to WebAssembly and served by Ahab; the",
		"highlighting, injection, tag and fold queries come from Helix.",
		"",
		`${rule}`,
		`Helix queries: ${sources.helix.repository}, commit ${commit}`,
		"Licensed under the Mozilla Public License 2.0: https://mozilla.org/MPL/2.0/",
		"The query files Ahab ships (as changed by Ahab) are its source form;",
		"they are in Ahab's repository under apps/ui/languages/queries/.",
	];
	for (const [name, grammar] of Object.entries(sources.grammars)) {
		const text = texts[name] ?? previousTexts[name] ?? "";
		parts.push(
			rule,
			`tree-sitter grammar: ${name}`,
			`${grammar.repository} at ${grammar.revision}${grammar.subpath ? ` (${grammar.subpath})` : ""}, ${grammar.license}`,
			"",
			text.trim(),
		);
	}
	mkdirSync(join(ui, "public", "licenses"), { recursive: true });
	writeFileSync(join(ui, "public", "licenses", "languages.txt"), parts.join("\n") + "\n");

	const rows = Object.entries(sources.grammars).map(
		([name, grammar]) =>
			`| ${name} | [${grammar.repository.replace("https://github.com/", "")}](${grammar.repository}) | \`${grammar.revision.slice(0, 12)}\` | ${grammar.license} |`,
	);
	const section = [
		"<!-- languages:start (written by `make grammars-update`) -->",
		"",
		"### Tree-sitter grammars",
		"",
		"Compiled to WebAssembly into `apps/ui/languages/grammars/` and served by Ahab.",
		"Their licence texts are in `apps/ui/public/licenses/languages.txt`, which",
		"Ahab serves at `/licenses/languages.txt`.",
		"",
		"| Grammar | Repository | Revision | Licence |",
		"| --- | --- | --- | --- |",
		...rows,
		"",
		"### Highlighting queries",
		"",
		`From [Helix](${sources.helix.repository}) at \`${commit.slice(0, 12)}\`,`,
		"under the [Mozilla Public License 2.0](https://mozilla.org/MPL/2.0/), in",
		"`apps/ui/languages/queries/`. Ahab writes inherited queries out in place of",
		"Helix's `; inherits:` lines; each file says where it comes from.",
		"",
		"<!-- languages:end -->",
	].join("\n");
	const notices = join(root, "THIRD_PARTY.md");
	const current = existsSync(notices) ? readFileSync(notices, "utf8") : "";
	const marked = /<!-- languages:start[\s\S]*<!-- languages:end -->/;
	writeFileSync(
		notices,
		marked.test(current)
			? current.replace(marked, section)
			: `${current.trimEnd()}\n\n${section}\n`,
	);
}
