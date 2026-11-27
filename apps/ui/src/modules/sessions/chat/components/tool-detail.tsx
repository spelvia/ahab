import { cn } from "cn";
import { type ReactNode, useContext, useState } from "react";

import { useFile, locateEdit } from "@/modules/projects";
import {
	ChatFileActions,
	ChatRoot,
	useShownPath,
} from "@/modules/sessions/chat/components/chat-root";
import {
	diffPath,
	outputKind,
	shellSummary,
	unwrapShell,
} from "@/modules/sessions/chat/components/command-text";
import { Diff } from "@/modules/sessions/chat/components/diff-block";
import { TerminalOutput } from "@/modules/sessions/chat/components/terminal-output";
import { type ToolCallItem, workedFor } from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";
import {
	type HighlightAs,
	HighlightedCode,
	HighlightedLine,
	useHighlightedLines,
} from "@/shared/syntax/code";
import { CopyButton } from "@/shared/ui/copy-button";
import { toastError } from "@/shared/ui/toast";

type Detail = ToolCallItem["detail"];
type Other = Extract<Detail, { kind: "other" }>;
type OpenFile = (path: string, line?: number) => void;

function fields(input: unknown): Record<string, unknown> {
	return input && typeof input === "object" && !Array.isArray(input)
		? (input as Record<string, unknown>)
		: {};
}

function firstText(input: Record<string, unknown>, ...keys: string[]) {
	for (const key of keys)
		if (typeof input[key] === "string" && input[key]) return input[key] as string;
	return "";
}

export function toolTarget(detail: Detail) {
	switch (detail.kind) {
		case "command":
			return shellSummary(detail.command);
		case "notebookEdit":
			return [detail.path, detail.operation, detail.cellId].filter(Boolean).join(" · ");
		case "fileChange":
			return detail.changes.map((change) => change.path).join(", ");
		case "mcp":
		case "mcpCall":
			return detail.server + " · " + detail.tool;
		case "other": {
			const input = fields(detail.input);
			const target = firstText(
				input,
				"pattern",
				"query",
				"url",
				"file_path",
				"path",
				"skill",
				"description",
				"tool",
				"command",
			);
			return target ? detail.name + " · " + target : detail.name;
		}
	}
}

/** Provider file paths are literal names. Only open paths belonging to the chat project. */
export function projectPath(path: string, root?: string): string | undefined {
	if (!path || path.includes("\0") || path.startsWith("//")) return undefined;
	const base = root?.replace(/\/$/, "");
	if (path.startsWith("/") && (base === undefined || !path.startsWith(base + "/")))
		return undefined;
	if (/^(?:[A-Za-z]:[\\/]|\\\\)/.test(path)) {
		if (!root || !/^(?:[A-Za-z]:[\\/]|\\\\)/.test(root)) return undefined;
		const prefix = root.replaceAll("\\", "/").replace(/\/$/, "") + "/";
		const normalized = path.replaceAll("\\", "/");
		return normalized.toLowerCase().startsWith(prefix.toLowerCase())
			? projectPath(normalized.slice(prefix.length))
			: undefined;
	}
	const relative = path.startsWith("/") ? path.slice((base?.length ?? 0) + 1) : path;
	const parts: string[] = [];
	for (const part of relative.split("/")) {
		if (part === "." || !part) continue;
		if (part === "..") {
			if (!parts.length) return undefined;
			parts.pop();
		} else parts.push(part);
	}
	return parts.join("/") || undefined;
}

export function ToolFile({ path, line, open }: { path: string; line?: number; open?: OpenFile }) {
	const root = useContext(ChatRoot);
	const shown = useShownPath();
	const target = projectPath(path, root);
	const label = shown(path) + (line ? `:${line}` : "");
	return open && target ? (
		<button
			type="button"
			className="text-left font-mono break-all hover:underline"
			onClick={() => open(target, line)}
		>
			{label}
		</button>
	) : (
		<span className="font-mono break-all">{label}</span>
	);
}

export function OpenChange({ path }: { path: string }) {
	const actions = useContext(ChatFileActions);
	const root = useContext(ChatRoot);
	const { t } = useTranslation("sessions");
	const target = projectPath(path, root);
	if (!target || !actions.diff) return null;
	return (
		<button
			type="button"
			className="ml-3 text-xs hover:underline"
			onClick={() => void actions.diff?.(target).catch(toastError)}
		>
			{t("tools.openDiff")}
		</button>
	);
}

export function UnnumberedHunk({
	path,
	body,
	open,
}: {
	path: string;
	body: string[];
	open: OpenFile;
}) {
	const actions = useContext(ChatFileActions);
	const root = useContext(ChatRoot);
	const { t } = useTranslation("sessions");
	const target = projectPath(path, root);
	return actions.project && target ? (
		<button
			type="button"
			className="ml-2 underline"
			onClick={() => {
				void locateEdit(actions.project!, target, body)
					.then((line) => open(target, line))
					.catch(toastError);
			}}
		>
			{t("tools.openHunk")}
		</button>
	) : null;
}

/** Lines of a created file shown at first; the rest on request. */
const CREATED_LINES = 400;

export function CreatedCode({ text, path, open }: { text: string; path: string; open?: OpenFile }) {
	const { t } = useTranslation("sessions");
	const [all, setAll] = useState(false);
	const lines = text.split("\n");
	// Stubb emits plus-prefixed bodies for Claude and plain content for some adapters.
	const body = lines.filter((line) => line !== "");
	const code =
		body.length && body.every((line) => line.startsWith("+"))
			? lines.map((line) => line.slice(1)).join("\n")
			: text;
	const tokens = useHighlightedLines(code.length > HIGHLIGHT_MAX ? undefined : code, { path });
	const codeLines = code.split("\n");
	const shown = all ? codeLines : codeLines.slice(0, CREATED_LINES);
	return (
		<div>
			<div
				tabIndex={0}
				role="region"
				aria-label={path}
				className="bg-muted/60 max-h-96 overflow-auto rounded-md py-2 font-mono text-xs leading-5"
			>
				{shown.map((line, i) => (
					// oxlint-disable-next-line react/no-array-index-key -- positions belong to this ordered snapshot.
					<div key={i} className="flex whitespace-pre">
						<span className="text-muted-foreground px-2 select-none" title={path}>
							{open ? <LineLink path={path} line={i + 1} open={open} focusable={false} /> : i + 1}
						</span>
						<code>
							<HighlightedLine text={line || " "} tokens={tokens?.[i]} />
						</code>
					</div>
				))}
			</div>
			{shown.length < codeLines.length && (
				<button type="button" className="mt-1 text-xs hover:underline" onClick={() => setAll(true)}>
					{t("tools.showLines", { count: codeLines.length })}
				</button>
			)}
		</div>
	);
}

export function LineLink({
	path,
	line,
	open,
	focusable = true,
}: {
	path: string;
	line: number;
	open: OpenFile;
	/** False where many sit together: their region is the one tab stop. */
	focusable?: boolean;
}) {
	const root = useContext(ChatRoot);
	const target = projectPath(path, root);
	return target ? (
		<button
			type="button"
			aria-label={path + ":" + line}
			tabIndex={focusable ? undefined : -1}
			className="hover:underline"
			onClick={() => open(target, line)}
		>
			{line}
		</button>
	) : (
		<>{line}</>
	);
}

/** Texts over this many characters are shown plain: highlighting them would cost the page too much. */
const HIGHLIGHT_MAX = 100_000;

function Code({ text, as }: { text: string; as: HighlightAs | undefined }) {
	return text.length > HIGHLIGHT_MAX ? <>{text}</> : <HighlightedCode text={text} as={as} />;
}

/** Lines of a command shown before its block scrolls; all of them on request. */
const COMMAND_LINES = 16;

/** A command as it runs: its shell text highlighted as shell, embedded scripts in their own language. */
function CommandText({ script }: { script: string }) {
	const { t } = useTranslation("sessions");
	const [all, setAll] = useState(false);
	const count = script.split("\n").length;
	return (
		<div>
			<pre
				tabIndex={0}
				role="region"
				aria-label={t("tools.commandText")}
				className={cn(
					"bg-muted/60 overflow-auto rounded-md px-3 py-2 font-mono text-xs leading-5 break-words whitespace-pre-wrap",
					!all && "max-h-80",
				)}
			>
				<Code text={script.replace(/^\n|\n$/g, "")} as={{ language: "bash" }} />
			</pre>
			{count > COMMAND_LINES && (
				<button type="button" className="mt-1 hover:underline" onClick={() => setAll(!all)}>
					{all ? t("tools.less") : t("tools.showLines", { count })}
				</button>
			)}
		</div>
	);
}

function CommandOutput({ text }: { text: string }) {
	const kind = outputKind(text);
	if (kind === "diff") return <Diff text={text} path={diffPath(text)} />;
	return (
		<pre tabIndex={0} className="bg-muted/60 max-h-80 overflow-auto rounded-md p-3">
			{kind === "json" ? (
				<HighlightedCode text={text} as={{ language: "json" }} />
			) : (
				<TerminalOutput text={text} />
			)}
		</pre>
	);
}

/** A block's name above it, with what acts on the block (copy) at its end. */
function BlockHeading({ title, children }: { title: string; children?: ReactNode }) {
	return (
		<div className="text-muted-foreground -mb-0.5 flex items-center justify-between gap-2 pt-1">
			<span className="text-[0.6875rem] font-medium tracking-wide uppercase">{title}</span>
			{children}
		</div>
	);
}

/** A command in full, as the agent ran it, with copy: the shell wrapper unwrapped. */
export function FullCommand({ command }: { command: string }) {
	const { t } = useTranslation("sessions");
	const { script } = unwrapShell(command);
	return (
		<div className="min-w-0 space-y-1.5 text-xs">
			<BlockHeading title={t("tools.commandText")}>
				<CopyButton text={script} label={t("tools.command")} />
			</BlockHeading>
			<CommandText script={script} />
		</div>
	);
}

export function CommandBody({
	detail,
	duration,
}: {
	detail: Extract<Detail, { kind: "command" }>;
	duration?: number | null;
}) {
	const { t } = useTranslation("sessions");
	const [all, setAll] = useState(false);
	const shown = useShownPath();
	const output = detail.output ?? "";
	const short = output.length > 8000 && !all;
	const { script, wrapper } = unwrapShell(detail.command);
	return (
		<div className="min-w-0 space-y-1.5 text-xs">
			<div className="text-muted-foreground flex flex-wrap items-center gap-2">
				{detail.cwd && shown(detail.cwd) !== "." && (
					<span title={detail.cwd}>{shown(detail.cwd)}</span>
				)}
				{duration != null && <span>{workedFor(duration)}</span>}
				{detail.exitCode != null && (
					<span>{t("conversation.exitCode", { code: detail.exitCode })}</span>
				)}
			</div>
			<BlockHeading title={t("tools.commandText")}>
				<CopyButton text={script} label={t("tools.command")} />
			</BlockHeading>
			<CommandText script={script} />
			{wrapper && (
				<details>
					<summary className="text-muted-foreground cursor-pointer">
						{t("tools.invocation", { wrapper })}
					</summary>
					<div className="mt-1 flex items-start gap-1">
						<pre className="bg-muted/60 max-h-60 min-w-0 flex-1 overflow-auto rounded-md px-3 py-2 font-mono break-all whitespace-pre-wrap">
							{detail.command}
						</pre>
						<CopyButton text={detail.command} label={t("tools.invocationCopy")} />
					</div>
				</details>
			)}
			{output ? (
				<>
					<BlockHeading title={t("tools.outputText")}>
						<CopyButton text={output} label={t("tools.output")} />
					</BlockHeading>
					<CommandOutput text={short ? output.slice(-8000) : output} />
				</>
			) : (
				<p className="text-muted-foreground">{t("conversation.noOutput")}</p>
			)}
			{short && (
				<button type="button" onClick={() => setAll(true)} className="hover:underline">
					{t("tools.showAll")}
				</button>
			)}
		</div>
	);
}

export function OtherBody({ detail, open }: { detail: Other; open?: OpenFile }) {
	const input = fields(detail.input);
	const path = firstText(input, "file_path", "path", "notebook_path");
	const { t } = useTranslation("sessions");
	const url = firstText(input, "url");
	const search = detail.name === "Grep" || detail.name === "Glob";
	const web = detail.name === "WebSearch" || detail.name === "WebFetch";
	const results =
		web && detail.output
			? Array.from(new Set(detail.output.match(/https?:\/\/[^\s<>"\])]+/g) ?? []))
			: [];
	const known = [
		"Read",
		"Grep",
		"Glob",
		"WebSearch",
		"WebFetch",
		"Skill",
		"Task",
		"Agent",
		"TodoWrite",
	].includes(detail.name);
	return (
		<div className="space-y-2 text-xs">
			{path && (
				<ToolFile
					path={path}
					line={typeof input.offset === "number" && input.offset > 0 ? input.offset : undefined}
					open={open}
				/>
			)}
			{detail.name === "Read" && typeof input.limit === "number" && input.limit > 0 && (
				<span className="text-muted-foreground ml-2">
					{t("tools.requestedLines", { count: input.limit })}
				</span>
			)}
			{url && /^https?:\/\//i.test(url) && (
				<a className="break-all underline" href={url} target="_blank" rel="noreferrer">
					{url}
				</a>
			)}
			{results.length > 0 && (
				<ul>
					{results.map((link) => (
						<li key={link}>
							<a className="break-all underline" href={link} target="_blank" rel="noreferrer">
								{link}
							</a>
						</li>
					))}
				</ul>
			)}
			{search && detail.output && (
				<p>{t("tools.resultLines", { count: detail.output.trim().split("\n").length })}</p>
			)}
			{detail.output && (
				<pre className="bg-muted/60 max-h-80 overflow-auto rounded p-2">
					{search ? (
						detail.output.split("\n").map((line, i) => {
							const match = /^(.+?):(\d+)(?::(.*))?$/.exec(line);
							return (
								// oxlint-disable-next-line react/no-array-index-key -- positions belong to this ordered snapshot.
								<div key={i}>
									{match?.[1] ? (
										<>
											<ToolFile path={match[1]} line={Number(match[2])} open={open} />
											{match[3] && ": " + match[3]}
										</>
									) : detail.name === "Glob" && line ? (
										<ToolFile path={line} open={open} />
									) : (
										line
									)}
								</div>
							);
						})
					) : detail.name === "Read" && path ? (
						<HighlightedCode text={detail.output} as={{ path }} />
					) : (
						detail.output
					)}
				</pre>
			)}
			{detail.input != null && (
				<details open={!known}>
					<summary className="text-muted-foreground cursor-pointer">{t("tools.input")}</summary>
					<pre className="max-h-60 overflow-auto">
						<HighlightedCode
							text={JSON.stringify(detail.input, null, 2)}
							as={{ language: "json" }}
						/>
					</pre>
				</details>
			)}
		</div>
	);
}

export function NotebookCell({ detail }: { detail: Extract<Detail, { kind: "notebookEdit" }> }) {
	const project = useContext(ChatFileActions).project;
	const path = projectPath(detail.path, useContext(ChatRoot));
	const file = useFile(
		project ?? "",
		path ?? "",
		Boolean(project && path && detail.cellType !== "markdown"),
	);
	let language = detail.cellType === "markdown" ? "markdown" : undefined;
	if (!language && file.data?.content) {
		try {
			const notebook = JSON.parse(file.data.content);
			const name =
				notebook.metadata?.language_info?.name ?? notebook.metadata?.kernelspec?.language;
			if (typeof name === "string") language = name;
		} catch {
			/* Some notebooks exceed the file preview limit. */
		}
	}
	return (
		<pre tabIndex={0} className="bg-muted/60 max-h-80 overflow-auto p-2 text-xs">
			<HighlightedCode text={detail.source} as={language ? { language } : undefined} />
		</pre>
	);
}
