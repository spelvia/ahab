/** Where a link in a project's file leads. */
interface FileLink {
	/** Another file, relative to the project; absent for the same file. */
	path?: string;
	/** A heading's anchor in it. */
	anchor?: string;
}

/**
 * Resolves a link written in the file at `from` (relative to the
 * project): `002-daemon.md`, `../docs/a.md#usage`, `/rfcs/005-git.md`
 * (from the project's root) or `#open-questions`. Links that leave the
 * project (a scheme, or above its root) resolve to nothing.
 */
export function resolveLink(from: string, href: string): FileLink | undefined {
	if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("//")) return undefined;
	const [target = "", fragment] = href.split("#", 2);
	let path: string;
	let anchor: string | undefined;
	try {
		path = decodeURIComponent(target.split("?")[0] ?? "");
		anchor = fragment ? decodeURIComponent(fragment) : undefined;
	} catch {
		return undefined;
	}
	if (!path) return { anchor };
	const parts = path.startsWith("/") ? [] : from.split("/").slice(0, -1);
	for (const part of path.split("/")) {
		if (!part || part === ".") continue;
		if (part === "..") {
			if (parts.length === 0) return undefined;
			parts.pop();
		} else parts.push(part);
	}
	const resolved = parts.join("/");
	return resolved === from ? { anchor } : { path: resolved, anchor };
}

/** Convert agent-written links into encoded project-relative destinations. */
export function chatFileHref(href: string, root?: string): string | undefined {
	if (/^file:\/\/(?:localhost)?\/(?!\/)/i.test(href)) {
		href = href.replace(/^file:\/\/(?:localhost)?/i, "");
	}
	// A root-level filename followed by :line can resemble a URL scheme.
	if (/^[\w.-]+:\d+(?::\d+)?$/.test(href)) href = `./${href}`;
	const link = resolveLink("", href);
	if (!link?.path) return undefined;
	let path = link.path;
	if (href.startsWith("/")) {
		if (!root) return undefined;
		const prefix = root.replace(/\/$/, "");
		if (!`/${path}`.startsWith(`${prefix}/`)) return undefined;
		path = path.slice(prefix.length);
	}
	const position = /:(\d+)(?::\d+)?$/.exec(path);
	const line = /^L(\d+)(?:C\d+)?(?:-L?\d+)?$/i.exec(link.anchor ?? "")?.[1] ?? position?.[1];
	if (position) path = path.slice(0, position.index);
	if (!path) return undefined;
	return `./${path.split("/").map(encodeURIComponent).join("/")}${line ? `#L${line}` : ""}`;
}

/** Extensions a bare file name (no folder) must have to be taken for a file: not `example.com`. */
const FILE_EXTENSIONS = new Set(
	(
		"c cc cpp cs css cxx dart ex exs go h hpp hs htm html java js json jsonc jsx kt kts lock " +
		"lua m md mdx mjs cjs ml nix php pl proto py pyi rb rs scala scss sh sql svelte swift tex " +
		"toml ts tsx txt vue xml yaml yml zig zsh bash fish ps1 gradle cfg conf ini env graphql " +
		"gql tf hcl rst ipynb csv mk cmake dockerfile"
	).split(" "),
);

/**
 * Whether `path:line` written in prose is a file location: it has a
 * folder (`src/a.rs:3`), or its name has a known file extension
 * (`main.rs:3`). `api.example.com:443` is a host and port.
 */
export function isFileLocation(location: string): boolean {
	const path = location.replace(/(?::\d+){1,2}$/, "");
	if (path.includes("/")) return true;
	const extension = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
	return path.includes(".") && FILE_EXTENSIONS.has(extension);
}

/** Turn bare source locations in chat prose into the same links as Markdown file links. */
export function chatLocations() {
	type Node = {
		type: string;
		tagName?: string;
		value?: string;
		properties?: { href: string };
		children?: Node[];
	};
	function visit(node: Node) {
		if (!node.children || ["a", "code", "pre", "math", "svg"].includes(node.tagName ?? "")) return;
		node.children = node.children.flatMap((child) => {
			if (child.type !== "text" || !child.value) {
				visit(child);
				return [child];
			}
			const text = child.value;
			const parts: Node[] = [];
			let last = 0;
			for (const match of text.matchAll(
				/(?:^|[\s(])((?:[./\w-]+\/)*[\w.-]+\.[a-zA-Z][\w-]*:\d+(?::\d+)?)(?=$|[\s),.;!?])/g,
			)) {
				const location = match[1];
				if (!location || !isFileLocation(location)) continue;
				const start = match.index + match[0].length - location.length;
				parts.push(
					{ type: "text", value: text.slice(last, start) },
					{
						type: "element",
						tagName: "a",
						properties: { href: location },
						children: [{ type: "text", value: location }],
					},
				);
				last = start + location.length;
			}
			return last ? [...parts, { type: "text", value: text.slice(last) }] : [child];
		});
	}
	return visit;
}
