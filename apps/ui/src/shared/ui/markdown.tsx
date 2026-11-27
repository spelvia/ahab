import { cn } from "cn";
import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import ReactMarkdown, { type Components, defaultUrlTransform } from "react-markdown";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import rehypeSlug from "rehype-slug";
import remarkGfm from "remark-gfm";

import { useTranslation } from "@/shared/i18n";
import { HighlightedCode } from "@/shared/syntax/code";
import { CopyButton } from "@/shared/ui/copy-button";
import { ImageViewer } from "@/shared/ui/image-viewer";
import { markdownImages, markdownImageUrl } from "@/shared/ui/markdown-images";
import { chatLocations, chatFileHref, resolveLink } from "@/shared/ui/markdown-links";
import { mayHoldMath, normalizeMath } from "@/shared/ui/markdown-math";

type Plugins = NonNullable<React.ComponentProps<typeof ReactMarkdown>["remarkPlugins"]>;
type MathPlugins = { remark: Plugins; rehype: Plugins };

const remarkPlugins = [remarkGfm];
// Headings get GitHub's anchors: "Open questions" → `#open-questions`.
const rehypePlugins: Plugins = [markdownImages, rehypeSlug];
// A document's HTML, as GitHub shows it (`<p align="center"><img …>`,
// `<details>`): parsed, then sanitized with GitHub's rules (no scripts, no
// event handlers, no styles). Anchors come after, unprefixed.
const htmlPlugins: Plugins = [rehypeRaw, [rehypeSanitize, defaultSchema], rehypeSlug];

let mathLoaded: MathPlugins | undefined;
let mathLoading: Promise<MathPlugins> | undefined;

/** KaTeX and its plugins, loaded once, when some text first may hold math. */
function loadMath(): Promise<MathPlugins> {
	mathLoading ??= Promise.all([
		import("remark-math"),
		import("rehype-katex"),
		import("katex/dist/katex.min.css"),
	]).then(([math, katex]) => {
		mathLoaded = {
			remark: [remarkGfm, math.default],
			// A mistake in the math shows the source in red, not an error.
			rehype: [[katex.default, { throwOnError: false, strict: false }]],
		};
		return mathLoaded;
	});
	return mathLoading;
}

/** The math plugins once loaded, when `text` may hold math. */
function useMath(text: string, streaming: boolean): MathPlugins | undefined {
	const needed = !streaming && mayHoldMath(text);
	const [plugins, setPlugins] = useState(mathLoaded);
	useEffect(() => {
		if (needed && !plugins) void loadMath().then(setPlugins);
	}, [needed, plugins]);
	return needed ? plugins : undefined;
}

/** A link that leaves the app: it has a scheme (`https:`, `mailto:`). */
function leaves(href: string): boolean {
	return /^[a-z][a-z0-9+.-]*:/i.test(href);
}

/** Who may take a link that stays in the app; see {@link Markdown}. */
const Links = createContext<((href: string) => boolean) | undefined>(undefined);
const ProjectLinks = createContext(false);

function Link({ node: _node, href, ...props }: React.ComponentProps<"a"> & { node?: unknown }) {
	const onLink = useContext(Links);
	const { t } = useTranslation();
	const projectLinks = useContext(ProjectLinks);
	if (!href && projectLinks) {
		return (
			<span>
				{props.children}{" "}
				<small className="text-muted-foreground">({t("errors.unavailableProjectLink")})</small>
			</span>
		);
	}
	if (href && (leaves(href) || href.startsWith("//")))
		return <a href={href} {...props} target="_blank" rel="noreferrer" />;
	return (
		<a
			href={href || undefined}
			{...props}
			onClick={(event) => {
				if (href && onLink?.(href)) event.preventDefault();
			}}
		/>
	);
}

/** A code block in a language (```rust) is highlighted as the editor would. */
function Code({
	node: _node,
	className,
	children,
	streaming,
	...props
}: React.ComponentProps<"code"> & { node?: unknown; streaming?: boolean }) {
	const language = /\blanguage-([^\s]+)/.exec(className ?? "")?.[1];
	if (streaming || !language || typeof children !== "string")
		return (
			<code className={className} {...props}>
				{children}
			</code>
		);
	return (
		<code className={className} data-language={language} {...props}>
			<HighlightedCode text={children.replace(/\n$/, "")} as={{ language }} />
		</code>
	);
}

/**
 * An image opens over the whole window, to zoom and move around in, with a
 * click or Enter. One inside a link stays the link's.
 */
function Image({
	node: _node,
	src,
	alt,
	className,
	...props
}: React.ComponentProps<"img"> & { node?: unknown }) {
	const { t } = useTranslation();
	const [open, setOpen] = useState(false);
	const [linked, setLinked] = useState(false);
	const measure = useCallback((element: HTMLImageElement | null) => {
		if (element) setLinked(Boolean(element.closest("a")));
	}, []);
	if (typeof src !== "string" || !src)
		return <img src={undefined} alt={alt} className={className} {...props} />;
	return (
		<>
			<img
				src={src}
				alt={alt}
				{...props}
				ref={measure}
				className={cn(!linked && "cursor-zoom-in", className)}
				role={linked ? undefined : "button"}
				tabIndex={linked ? undefined : 0}
				aria-label={
					linked
						? undefined
						: alt
							? t("imageViewer.open", { label: alt })
							: t("imageViewer.openUnnamed")
				}
				onClick={(event) => {
					if (linked) return;
					event.preventDefault();
					setOpen(true);
				}}
				onKeyDown={(event) => {
					if (linked || (event.key !== "Enter" && event.key !== " ")) return;
					event.preventDefault();
					setOpen(true);
				}}
			/>
			{open && <ImageViewer src={src} alt={alt} open onOpenChange={setOpen} />}
		</>
	);
}

function CodeBlock({ node, children }: ComponentPropsForPre) {
	const { t } = useTranslation();
	const view = useRef<HTMLPreElement>(null);
	const [wide, setWide] = useState(false);
	useEffect(() => {
		const box = view.current;
		if (!box || typeof ResizeObserver === "undefined") return;
		const measure = () => setWide(box.scrollWidth > box.clientWidth + 1);
		const observer = new ResizeObserver(measure);
		observer.observe(box);
		measure();
		return () => observer.disconnect();
	}, []);
	const code = node?.children.find((child) => child.type === "element" && child.tagName === "code");
	if (code?.type !== "element") return <pre>{children}</pre>;
	const language = String(code.properties.className ?? "").match(/language-([^, ]+)/)?.[1];
	const source = code.children
		.map((child) => (child.type === "text" ? child.value : ""))
		.join("")
		.replace(/\n$/, "");
	return (
		<div className="min-w-0">
			<div className="text-muted-foreground flex items-center justify-end gap-2 text-xs">
				{wide && <span className="mr-auto">{t("codeBlock.scroll")}</span>}
				{language && <span>{language}</span>}
				<CopyButton text={source} label={language ?? "code"} />
			</div>
			<pre ref={view} tabIndex={0}>
				{children}
			</pre>
		</div>
	);
}
type ComponentPropsForPre = React.ComponentProps<"pre"> & import("react-markdown").ExtraProps;

const components: Components = { a: Link, code: Code, img: Image, pre: CodeBlock };
const streamingComponents: Components = {
	...components,
	pre: "pre",
	code: (props) => <Code {...props} streaming />,
};

/**
 * Markdown as agents and people write it (GitHub flavoured), styled for the
 * app; raw HTML is rendered, sanitized as GitHub does, only with `html` (a
 * document, not a chat: there a stray `<div>` is text); a lone `<img>` always is; code blocks are highlighted; math (`$…$`,
 * `$$…$$`, `\(…\)`, `\[…\]`) is typeset with KaTeX, loaded when first needed. Headings get GitHub's anchors. `onLink`
 * may take a link that stays in the app (a file, `#a-heading`); when it
 * returns true the browser does not follow it. Links that leave open in a
 * new tab.
 */
export function Markdown({
	text,
	className,
	onLink,
	files,
	imageBase,
	html = false,
	streaming = false,
}: {
	text: string;
	/** Render the text's HTML, sanitized (a file's preview). */
	html?: boolean;
	/** Defer syntax highlighting and typesetting until the agent finishes this message. */
	streaming?: boolean;
	className?: string;
	imageBase?: { project: string; path: string; root?: string };
	onLink?: (href: string) => boolean;
	files?: { root?: string; open: (path: string, line?: number) => void };
}) {
	const math = useMath(text, streaming);
	const open = files?.open;
	const followFile = useCallback(
		(href: string) => {
			if (!open || !href.startsWith("./")) return false;
			const link = resolveLink("", href);
			if (!link?.path) return false;
			open(link.path, link.anchor ? Number(link.anchor.slice(1)) : undefined);
			return true;
		},
		[open],
	);
	return (
		<ProjectLinks.Provider value={files !== undefined}>
			<Links.Provider value={files ? followFile : onLink}>
				<div className={cn("markdown", className)}>
					<ReactMarkdown
						remarkPlugins={math?.remark ?? remarkPlugins}
						rehypePlugins={[
							...(html ? htmlPlugins : rehypePlugins),
							...(math?.rehype ?? []),
							...(files && !streaming ? [chatLocations] : []),
						]}
						components={streaming ? streamingComponents : components}
						urlTransform={
							files || imageBase
								? (url, key, node) => {
										if (imageBase && key === "src" && node.tagName === "img") {
											return markdownImageUrl(
												imageBase.project,
												imageBase.path,
												url,
												imageBase.root,
											);
										}
										if (files && key === "href" && node.tagName === "a" && !url.startsWith("#")) {
											const local = chatFileHref(url, files.root);
											if (local) return local;
											if ((!leaves(url) && !url.startsWith("//")) || url.startsWith("file:")) {
												return "";
											}
										}
										return defaultUrlTransform(url);
									}
								: undefined
						}
					>
						{math ? normalizeMath(text) : text}
					</ReactMarkdown>
				</div>
			</Links.Provider>
		</ProjectLinks.Provider>
	);
}
