#!/usr/bin/env node
// Checks the UI's layout and import rules, which mirror the server's:
// the file names modules and components may use, which modules may use
// which, that modules are reached only through their index.ts, that shared/
// knows no module, that only a component's api.ts calls the generated
// client, that only shared/preferences touches localStorage, and that routes
// stay thin.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const src = fileURLToPath(new URL("../src", import.meta.url));

/** What each module may use besides shared/, as on the server. */
const uses = {
	platform: [],
	clients: [],
	machines: ["platform"],
	projects: ["platform", "machines"],
	sessions: ["platform", "machines", "projects"],
	plugins: ["platform", "machines", "projects", "sessions"],
	tools: ["platform", "machines", "projects", "sessions"],
	native: [],
	notifications: [],
};
/** A component's own files; each may instead be a folder of parts with an index.ts. */
const componentFiles = new Set(["index.ts", "api.ts", "stream.ts", "model.ts", "hooks.ts"]);
const componentFolders = new Set(["api", "stream", "model", "hooks", "pages", "components"]);
const splitFolders = new Set(["api", "stream", "model", "hooks"]);
const topLevel = new Set([
	"main.tsx",
	"routeTree.gen.ts",
	"app",
	"routes",
	"shared",
	"modules",
	"testing",
]);
const ROUTE_LINES = 60;

const violations = [];
const fail = (file, message) => violations.push(`${relative(src, file)}: ${message}`);

checkNames();
for (const file of walk(src)) checkFile(file);

if (violations.length > 0) {
	for (const violation of violations) console.error(violation);
	console.error(`${violations.length} layout rule violation(s); see AGENTS.md`);
	process.exit(1);
}
console.log("layout rules ok");

/** Rule 1: the names of the top level, modules and components. */
function checkNames() {
	for (const entry of readdirSync(src)) {
		if (!topLevel.has(entry)) fail(join(src, entry), "not a place for code at the top of src/");
	}
	const modules = join(src, "modules");
	for (const module of readdirSync(modules)) {
		const moduleDir = join(modules, module);
		if (!(module in uses)) fail(moduleDir, "not a module");
		for (const entry of readdirSync(moduleDir)) {
			const path = join(moduleDir, entry);
			if (entry === "index.ts") continue;
			if (!statSync(path).isDirectory())
				fail(path, "a module holds only index.ts and component folders");
			else checkComponent(path);
		}
		if (!readdirSync(moduleDir).includes("index.ts")) fail(moduleDir, "a module needs an index.ts");
	}
}

function checkComponent(dir) {
	if (!readdirSync(dir).includes("index.ts")) fail(dir, "a component needs an index.ts");
	for (const entry of readdirSync(dir)) {
		const path = join(dir, entry);
		if (statSync(path).isDirectory()) {
			if (!componentFolders.has(entry))
				fail(path, `not a component folder (${[...componentFolders].join(", ")})`);
			else if (splitFolders.has(entry)) {
				if (readdirSync(dir).includes(`${entry}.ts`)) fail(path, `both ${entry}.ts and ${entry}/`);
				if (!readdirSync(path).includes("index.ts"))
					fail(path, "a split file's folder needs an index.ts");
			}
			continue;
		}
		const tested = entry.replace(/\.test\.tsx?$/, ".ts");
		if (!componentFiles.has(entry) && !componentFiles.has(tested))
			fail(
				path,
				`not a component file (${[...componentFiles].join(", ")}, their tests, ${[...componentFolders].join("/, ")}/)`,
			);
	}
}

function checkFile(file) {
	const path = relative(src, file).split(sep);
	const text = readFileSync(file, "utf8");
	const test = /\.test\.tsx?$/.test(file) || path[0] === "testing";
	const here = place(path);

	if (path.at(-1) === "index.ts") {
		if (path[0] !== "modules" || (path.length > 4 && !splitFolders.has(path[3])))
			fail(file, "index.ts only for modules, components and split files");
		const code = text
			.replace(/\/\/.*$/gm, "")
			.replace(/\/\*[\s\S]*?\*\//g, "")
			.replace(/export\s+(type\s+)?(\*|\{[^}]*\})\s+from\s+["'][^"']+["'];?/g, "")
			.trim();
		if (code) fail(file, "an index.ts only re-exports");
	}

	// Rule 8: localStorage only through shared/preferences.
	if (
		!test &&
		/\blocalStorage\b/.test(text) &&
		!(path[0] === "shared" && path[1] === "preferences")
	)
		fail(file, "uses localStorage; keep it in shared/preferences");

	// Rule 6: routes stay thin.
	if (path[0] === "routes") {
		const lines = text.split("\n").length - 1;
		if (lines > ROUTE_LINES)
			fail(
				file,
				`${lines} lines; a route holds params, loaders and layout (at most ${ROUTE_LINES})`,
			);
		if (/\b(useQuery|useQueries|useMutation|useSuspenseQuery|useQueryClient)\b/.test(text))
			fail(file, "calls the API in a route; that belongs to a module's api.ts");
	}

	for (const specifier of importsOf(text)) {
		const target = targetOf(file, specifier);
		if (!target) continue;
		const there = place(target);

		// Rule 5: only api.ts calls the generated client, and only its own tag.
		if (target[0] === "shared" && target[1] === "api" && target[2] === "generated") {
			const types = target[3]?.startsWith("ahab.schemas");
			if (!types && !test) {
				const tag = here.module && `${here.module}-${here.component}`;
				if (!here.isApi) fail(file, `imports the generated client; only api.ts may (${specifier})`);
				else if (target[3] !== tag)
					fail(
						file,
						`imports another component's client (${target[3]}); use its module's index.ts`,
					);
			}
			continue;
		}

		// Rule 4: shared/ knows no module, route or app.
		if (path[0] === "shared" && ["modules", "routes", "app"].includes(target[0]))
			fail(file, `shared code imports ${target[0]}/ (${specifier})`);

		// Rule 7: model files use no React and no API.
		if (here.isModel && !test) {
			if (/^react($|-dom)|^@tanstack\/react-/.test(specifier))
				fail(file, `a model imports React (${specifier})`);
			if (there.isApi) fail(file, `a model imports the API (${specifier})`);
		}

		if (target[0] !== "modules" || !there.module) continue;
		// Modules use each other only in their direction.
		if (path[0] === "modules" && here.module !== there.module) {
			if (!uses[here.module]?.includes(there.module))
				fail(file, `the ${here.module} module may not use ${there.module} (${specifier})`);
			// Rule 3: other modules only through their index.ts.
			if (target.length !== 2 && !(target.length === 3 && target[2] === "index.ts"))
				fail(file, `reaches into ${there.module}; import from "@/modules/${there.module}"`);
		} else if (path[0] === "modules" && here.component !== there.component) {
			// Inside a module, components use each other's index.ts.
			if (target.length !== 3 && !(target.length === 4 && target[3] === "index.ts"))
				fail(
					file,
					`reaches into ${there.component}; import from "@/modules/${there.module}/${there.component}"`,
				);
		} else if (
			path[0] !== "modules" &&
			target.length > 2 &&
			!(target.length === 3 && target[2] === "index.ts")
		) {
			fail(file, `reaches into ${there.module}; import from "@/modules/${there.module}"`);
		}
	}
}

/** Which module and component a path is in, and what kind of file it is. */
function place(path) {
	if (path[0] !== "modules") return {};
	const [, module, component, part] = path;
	const name = part?.replace(/\.(test\.)?tsx?$/, "");
	return {
		module,
		component: component?.replace(/\.tsx?$/, ""),
		isApi: name === "api",
		isModel: name === "model",
	};
}

function importsOf(text) {
	const found = [];
	for (const [, specifier] of text.matchAll(/(?:from|import)\s*\(?\s*["']([^"']+)["']/g))
		found.push(specifier);
	return found;
}

/** The path inside src/ an import points at, split; nothing for packages. */
function targetOf(file, specifier) {
	let path;
	if (specifier.startsWith("@/")) path = join(src, specifier.slice(2));
	else if (specifier.startsWith(".")) path = resolve(dirname(file), specifier);
	else return /^react|^@tanstack\/react-/.test(specifier) ? [specifier] : undefined;
	const parts = relative(src, path).split(sep);
	return parts[0] === ".." ? undefined : parts;
}

function walk(dir) {
	return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
		const path = join(dir, entry.name);
		if (entry.isDirectory()) return path.endsWith(join("api", "generated")) ? [] : walk(path);
		return /\.(ts|tsx)$/.test(entry.name) ? [path] : [];
	});
}
