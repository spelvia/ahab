#!/usr/bin/env node
// `make tools-update`: asks each tool's source for its latest
// version, pins it in the tool's record (`tools/registry/<id>.json`), and
// records the SHA-256 of every release file. The change is left for review.
//
//   node scripts/tools-update.mjs [--only id,id] [--dry-run]
//
// npm, PyPI, crates.io, the Go proxy and GitHub releases are asked over
// HTTPS; GitHub's API is used with GITHUB_TOKEN (or `gh auth token`) when
// there is one. A release file GitHub gives no digest for is downloaded and
// hashed. Nothing here runs in tests.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const registry = join(root, "tools", "registry");

const args = process.argv.slice(2);
const only = (() => {
	const at = args.indexOf("--only");
	return at === -1 ? null : new Set(args[at + 1].split(","));
})();
const dryRun = args.includes("--dry-run");

const USER_AGENT = "ahab-tools-update (https://github.com/spelvia/ahab)";

function githubToken() {
	if (process.env.GITHUB_TOKEN) return process.env.GITHUB_TOKEN;
	try {
		return execFileSync("gh", ["auth", "token"], { encoding: "utf8" }).trim() || null;
	} catch {
		return null;
	}
}
const token = githubToken();

async function get(url, { json = true, github = false } = {}) {
	const headers = { "user-agent": USER_AGENT };
	if (github && token) headers.authorization = `Bearer ${token}`;
	if (github) headers.accept = "application/vnd.github+json";
	const response = await fetch(url, { headers, redirect: "follow" });
	if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
	return json ? response.json() : response;
}

async function sha256Of(url) {
	const response = await get(url, { json: false });
	const hash = createHash("sha256");
	for await (const chunk of response.body) hash.update(chunk);
	return hash.digest("hex");
}

const fill = (template, version) => template.replaceAll("${version}", version);

async function npmLatest(name, major) {
	if (major === undefined) {
		const answer = await get(`https://registry.npmjs.org/${name.replace("/", "%2F")}/latest`);
		return answer.version;
	}
	// Within a major version: a server pins the major of what it drives
	// (typescript-language-server needs TypeScript's tsserver).
	const answer = await get(`https://registry.npmjs.org/${name.replace("/", "%2F")}`);
	const stable = Object.keys(answer.versions).filter(
		(version) => !version.includes("-") && version.split(".")[0] === major,
	);
	return stable.toSorted(compareVersions).at(-1);
}

/** The newest version of each source type; GitHub's also fills `assets`. */
const latest = {
	async npm(source) {
		source.version = await npmLatest(source.package);
		if (source.extra) {
			source.extra = await Promise.all(
				source.extra.map(async (pinned) => {
					const at = pinned.lastIndexOf("@");
					const name = at > 0 ? pinned.slice(0, at) : pinned;
					const major = at > 0 ? pinned.slice(at + 1).split(".")[0] : undefined;
					return `${name}@${await npmLatest(name, major)}`;
				}),
			);
		}
	},
	async pypi(source) {
		const answer = await get(`https://pypi.org/pypi/${source.package}/json`);
		source.version = answer.info.version;
	},
	async cargo(source) {
		if (source.git) {
			const repo = source.git.replace("https://github.com/", "");
			const release = await get(`https://api.github.com/repos/${repo}/releases/latest`, {
				github: true,
			});
			source.version = release.tag_name;
			return;
		}
		const answer = await get(`https://crates.io/api/v1/crates/${source.crate}`);
		source.version = answer.crate.max_stable_version;
	},
	async go(source) {
		const answer = await get(`https://proxy.golang.org/${source.module}/@latest`);
		source.version = answer.Version;
	},
	async githubRelease(source, record, problems) {
		const release = await get(`https://api.github.com/repos/${source.repo}/releases/latest`, {
			github: true,
		});
		const tagTemplate = source.tag ?? "${version}";
		const prefix = tagTemplate.slice(0, tagTemplate.indexOf("${version}"));
		const suffix = tagTemplate.slice(tagTemplate.indexOf("${version}") + "${version}".length);
		let version = release.tag_name;
		if (prefix && version.startsWith(prefix)) version = version.slice(prefix.length);
		if (suffix && version.endsWith(suffix)) version = version.slice(0, -suffix.length);
		source.version = version;
		const byName = new Map(release.assets.map((asset) => [asset.name, asset]));
		for (const [platform, asset] of Object.entries(source.assets)) {
			const name = fill(asset.file, version);
			const found = byName.get(name);
			if (!found) {
				delete asset.sha256;
				problems.push(`${record.id}: ${platform}: no file ${name} in ${release.tag_name}`);
				continue;
			}
			asset.sha256 = found.digest?.startsWith("sha256:")
				? found.digest.slice("sha256:".length)
				: await sha256Of(found.browser_download_url);
		}
	},
	async download(source, record, problems) {
		if (record.id === "jdtls") {
			const base = "https://download.eclipse.org/jdtls/milestones/";
			const listing = await (await get(base, { json: false })).text();
			const versions = [...listing.matchAll(/milestones\/(\d+\.\d+\.\d+)/g)].map((m) => m[1]);
			const newest = versions.toSorted(compareVersions).at(-1);
			const files = await (await get(`${base}${newest}/`, { json: false })).text();
			const file = files.match(/jdt-language-server-[\d.]+-\d+\.tar\.gz(?!\.)/)?.[0];
			if (!newest || !file) {
				problems.push("jdtls: cannot read the milestones");
				return;
			}
			source.version = newest;
			source.assets = { any: { url: `${base}${newest}/${file}` } };
		}
		for (const [platform, asset] of Object.entries(source.assets)) {
			try {
				asset.sha256 = await sha256Of(fill(asset.url, source.version));
			} catch (error) {
				delete asset.sha256;
				problems.push(`${record.id}: ${platform}: ${error.message}`);
			}
		}
	},
	async system() {},
};

function compareVersions(a, b) {
	const pa = a.split(".").map(Number);
	const pb = b.split(".").map(Number);
	for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
		const d = (pa[i] ?? 0) - (pb[i] ?? 0);
		if (d) return d;
	}
	return 0;
}

const problems = [];
const changed = [];
for (const name of readdirSync(registry)
	.filter((file) => file.endsWith(".json"))
	.toSorted()) {
	const path = join(registry, name);
	const before = readFileSync(path, "utf8");
	const record = JSON.parse(before);
	if (only && !only.has(record.id)) continue;
	const update = latest[record.source?.type];
	if (!update) {
		problems.push(`${record.id}: no way to update a ${record.source?.type ?? "missing"} source`);
		continue;
	}
	try {
		await update(record.source, record, problems);
	} catch (error) {
		problems.push(`${record.id}: ${error.message}`);
		continue;
	}
	const after = `${JSON.stringify(record, null, "\t")}\n`;
	if (after !== before) {
		changed.push(`${record.id} ${record.source.version ?? ""}`);
		if (!dryRun) writeFileSync(path, after);
	}
	console.log(`${record.id}: ${record.source.version ?? record.source.type}`);
}

console.log(changed.length ? `\nChanged:\n  ${changed.join("\n  ")}` : "\nNothing changed.");
if (problems.length) {
	console.log(`\nNot updated (check these records by hand):\n  ${problems.join("\n  ")}`);
	process.exitCode = 1;
}
