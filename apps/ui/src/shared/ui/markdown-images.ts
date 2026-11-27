import { chatFileHref, resolveLink } from "@/shared/ui/markdown-links";

export function projectImageUrl(project: string, path: string): string {
	return `/api/v1/projects/${encodeURIComponent(project)}/image?${new URLSearchParams({ path })}`;
}

export function markdownImageUrl(
	project: string,
	document: string,
	src: string,
	root?: string,
): string {
	if (/^https?:\/\//i.test(src) || src.startsWith("//")) return src;
	const target = document
		? resolveLink(document, src)
		: resolveLink("", chatFileHref(src, root) ?? "");
	return target?.path ? projectImageUrl(project, target.path) : "";
}

interface HtmlNode {
	type: string;
	value?: string;
	children?: HtmlNode[];
	tagName?: string;
	properties?: Record<string, string>;
}

/** READMEs commonly use a standalone HTML img to set a logo's dimensions. */
function visit(node: HtmlNode) {
	if (node.type === "raw" && node.value && /^\s*<img\s/i.test(node.value)) {
		const template = document.createElement("template");
		template.innerHTML = node.value.trim();
		const image = template.content.firstChild;
		if (template.content.childNodes.length === 1 && image instanceof HTMLImageElement) {
			const properties: Record<string, string> = {};
			for (const name of ["src", "alt", "title", "height", "width"]) {
				const value = image.getAttribute(name);
				if (value !== null) properties[name] = value;
			}
			node.type = "element";
			node.tagName = "img";
			node.properties = properties;
			delete node.value;
			node.children = [];
		}
	}
	node.children?.forEach(visit);
}

export function markdownImages() {
	return visit;
}
