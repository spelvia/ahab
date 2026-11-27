#!/usr/bin/env node
// `ahab-plugin build [--watch]`: builds the plugin in the current folder
// into `dist/`, as its manifest's entries name them. Run with Node 24 or
// later.
import { watch as watchFolder } from "node:fs";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { compile, optimize } from "@tailwindcss/node";
import { Scanner } from "@tailwindcss/oxide";
import * as esbuild from "esbuild";

/** Ahab's theme for Tailwind, shipped with this SDK. */
const THEME = fileURLToPath(new URL("../theme.css", import.meta.url));

/**
 * Modules Ahab's web app shares with web entries: each import becomes the
 * file Ahab serves for it, so a plugin never bundles React, zod or the SDK.
 */
const SHARED = {
	react: "react",
	"react/jsx-runtime": "react-jsx-runtime",
	"react-dom": "react-dom",
	"react-dom/client": "react-dom-client",
	zod: "zod",
	"@ahab/plugin/shared": "ahab-plugin-shared",
	"@ahab/plugin/web": "ahab-plugin-web",
};

/** Points the shared modules at Ahab's copies. */
const shared = {
	name: "ahab-shared",
	setup(build) {
		const names = Object.keys(SHARED).map((name) => name.replace(/[/.]/g, "\\$&"));
		build.onResolve({ filter: new RegExp(`^(${names.join("|")})$`) }, (args) => ({
			path: `/plugin-runtime/${SHARED[args.path]}.js`,
			external: true,
		}));
	},
};

/** Where each entry's source is, by convention. */
const SOURCES = {
	web: "src/web/index.tsx",
	daemon: "src/daemon/index.ts",
	server: "src/server/index.ts",
};

function fail(message) {
	console.error(`ahab-plugin: ${message}`);
	process.exit(1);
}

/** Files of one entry's own folder that another entry's bundle took in. */
function crossings(metafile, entry, root) {
	const others = Object.keys(SOURCES)
		.filter((name) => name !== entry)
		.map((name) => path.join(root, "src", name) + path.sep);
	return Object.keys(metafile.inputs)
		.map((input) => path.resolve(root, input))
		.filter((input) => others.some((other) => input.startsWith(other)));
}

function options(entry, root, outfile) {
	const common = {
		absWorkingDir: root,
		entryPoints: [path.join(root, SOURCES[entry])],
		outfile,
		bundle: true,
		format: "esm",
		sourcemap: "linked",
		metafile: true,
		logLevel: "warning",
		legalComments: "none",
	};
	if (entry === "web") {
		return {
			...common,
			platform: "browser",
			target: "es2023",
			jsx: "automatic",
			minify: true,
		};
	}
	return {
		...common,
		platform: "node",
		target: "node24",
		// Bundled CommonJS packages may still `require` Node's own modules.
		banner: {
			js: "import { createRequire as __ahabRequire } from 'node:module'; const require = __ahabRequire(import.meta.url);",
		},
	};
}

/**
 * The web entry's stylesheet: the Tailwind classes its source uses,
 * compiled against Ahab's theme, written beside the bundle (`dist/web.js`
 * gets `dist/web.css`). Ahab loads it with the entry. Only utilities:
 * Ahab's page already has its base styles and components.
 *
 * The utilities are scoped to the plugin's own content, which Ahab marks
 * with `data-ahab-plugin="<id>"` (views, windows, title-bar items, settings
 * page, and the dialogs and menus they open). Unscoped, a plugin's
 * `.grid-cols-1`, loaded after Ahab's stylesheet, would beat Ahab's own
 * `md:grid-cols-…` everywhere in the app.
 */
async function writeStyles(root, outfile, id) {
	const input = [
		'@import "tailwindcss/theme.css" layer(theme);',
		`@layer utilities { @scope ([data-ahab-plugin=${JSON.stringify(id)}]) { @tailwind utilities; } }`,
		`@import ${JSON.stringify(THEME)};`,
	].join("\n");
	const compiler = await compile(input, {
		base: path.dirname(THEME),
		onDependency: () => {},
	});
	const scanner = new Scanner({
		sources: [{ base: path.join(root, "src"), pattern: "**/*", negated: false }],
	});
	const css = optimize(compiler.build(scanner.scan()), { minify: true }).code;
	const file = outfile.replace(/\.[cm]?js$/, ".css");
	await writeFile(file, css);
	return path.relative(root, file);
}

/**
 * The settings' JSON Schema, from `src/shared/settings.ts` (its `settings`
 * export), written to `dist/settings.json` for the manifest to name.
 */
async function writeSettings(root, manifest, { quiet = false } = {}) {
	const source = path.join(root, "src/shared/settings.ts");
	try {
		await readFile(source);
	} catch {
		return;
	}
	const bundled = path.join(root, "dist", ".settings.mjs");
	await mkdir(path.dirname(bundled), { recursive: true });
	// Converted in the same bundle, with the plugin's own zod.
	await esbuild.build({
		absWorkingDir: root,
		stdin: {
			contents: [
				'import { settings } from "./src/shared/settings.ts";',
				'import { z } from "@ahab/plugin/shared";',
				'export default z.toJSONSchema(settings, { io: "input", unrepresentable: "any" });',
			].join("\n"),
			resolveDir: root,
			loader: "ts",
		},
		outfile: bundled,
		bundle: true,
		format: "esm",
		platform: "node",
		logLevel: "warning",
	});
	const schema = (await import(`${bundled}?t=${Date.now()}`)).default;
	await writeFile(
		path.join(root, "dist", "settings.json"),
		`${JSON.stringify(schema, null, "\t")}\n`,
	);
	await rm(bundled, { force: true });
	if (!quiet && manifest.settings !== "dist/settings.json")
		console.warn('ahab-plugin: add "settings": "dist/settings.json" to ahab-plugin.json');
}

/**
 * Writes the settings again whenever `src/shared` changes: the settings may
 * import from anywhere there, and no entry has to import them. Changes
 * close together are written once.
 */
function watchSettings(root, manifest) {
	const folder = path.join(root, "src/shared");
	let timer;
	let writing = Promise.resolve();
	try {
		watchFolder(folder, { recursive: true }, () => {
			clearTimeout(timer);
			timer = setTimeout(() => {
				writing = writing
					.then(() => writeSettings(root, manifest, { quiet: true }))
					.then(() => console.log("ahab-plugin: wrote dist/settings.json"))
					.catch((error) =>
						console.error(`ahab-plugin: the settings cannot be written: ${error.message}`),
					);
			}, 100);
		});
	} catch {
		// No src/shared: no settings to keep up to date.
	}
}

async function main() {
	const [command = "build", ...flags] = process.argv.slice(2);
	if (command !== "build") fail(`unknown command ${command}; use "build" or "build --watch"`);
	const watch = flags.includes("--watch");
	const root = process.cwd();
	let manifest;
	try {
		manifest = JSON.parse(await readFile(path.join(root, "ahab-plugin.json"), "utf8"));
	} catch (error) {
		fail(`no readable ahab-plugin.json here (${error.message})`);
	}
	if (typeof manifest.id !== "string" || !manifest.id) fail("the manifest has no id");
	const entries = Object.entries(manifest.entries ?? {});
	if (!entries.length) {
		// A plugin that only declares (languages only) has nothing to build.
		if (manifest.contributes) {
			console.log(
				"ahab-plugin: nothing to build; the manifest names no entries, only contributions",
			);
			return;
		}
		fail("the manifest names no entries");
	}
	// Bundles are ES modules; installed builds have no package.json of the
	// plugin's own, so each output folder says so itself.
	const folders = new Set(
		entries
			.filter(([, file]) => typeof file === "string")
			.map(([, file]) => path.dirname(path.join(root, file))),
	);
	for (const folder of folders) {
		await mkdir(folder, { recursive: true });
		await writeFile(path.join(folder, "package.json"), '{ "type": "module" }\n');
	}
	await writeSettings(root, manifest);
	if (watch) watchSettings(root, manifest);
	for (const [entry, file] of entries) {
		if (!SOURCES[entry]) fail(`unknown entry ${entry}`);
		if (typeof file !== "string")
			fail(`entry ${entry}: a binary per platform is not built by this tool`);
		const config = options(entry, root, path.join(root, file));
		const check = {
			name: "ahab-boundaries",
			setup(build) {
				build.onEnd(async (result) => {
					if (entry === "web" && !result.errors.length)
						await writeStyles(root, config.outfile, manifest.id);
					if (!result.metafile) return;
					const crossed = crossings(result.metafile, entry, root);
					if (crossed.length) {
						const list = crossed.map((input) => path.relative(root, input)).join(", ");
						console.error(
							`ahab-plugin: the ${entry} entry imports another entry's files: ${list}; share them through src/shared`,
						);
						if (!watch) process.exit(1);
					}
				});
			},
		};
		if (watch) {
			const plugins = entry === "web" ? [shared, check] : [check];
			const context = await esbuild.context({ ...config, plugins });
			await context.watch();
			console.log(`ahab-plugin: watching the ${entry} entry`);
		} else {
			await esbuild.build({ ...config, plugins: entry === "web" ? [shared, check] : [check] });
			console.log(`ahab-plugin: built ${file}${entry === "web" ? " and its stylesheet" : ""}`);
		}
	}
}

await main();
