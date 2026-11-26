import { cn } from "cn";

import { UnnumberedHunk, LineLink } from "@/modules/sessions/chat/components/tool-detail";
import { HighlightedLine, useUnifiedDiffTokens } from "@/shared/syntax/code";

/** A unified diff, its added and removed lines coloured, the code highlighted as its file's language. */
export function Diff({
	text,
	path,
	open,
}: {
	text: string;
	path?: string;
	open?: (path: string, line?: number) => void;
}) {
	const lines = numberedLines(text);
	const tokens = useUnifiedDiffTokens(text, path);
	return (
		<pre
			tabIndex={0}
			className="bg-muted/60 max-h-96 overflow-auto rounded-md py-2 font-mono text-xs leading-5"
		>
			{lines.map(({ line, number, position }) => {
				const header = line.startsWith("+++") || line.startsWith("---");
				const added = line.startsWith("+") && !header;
				const removed = line.startsWith("-") && !header;
				const lineTokens = tokens?.[number];
				return (
					<div
						key={number}
						className={cn(
							"px-3",
							added && "bg-success/12",
							removed && "bg-destructive/12",
							line.startsWith("@@") && "text-muted-foreground",
						)}
					>
						{line === "@@" && path && open && (
							<UnnumberedHunk path={path} open={open} body={hunkBody(lines, number)} />
						)}
						{position && path && open ? (
							<span className="text-muted-foreground mr-2">
								<LineLink path={path} line={position} open={open} />
							</span>
						) : null}
						{lineTokens?.length ? (
							<>
								<span className={cn(added && "text-success", removed && "text-destructive")}>
									{line.slice(0, 1)}
								</span>
								<HighlightedLine text={line.slice(1)} tokens={lineTokens} />
							</>
						) : (
							<span className={cn(added && "text-success", removed && "text-destructive")}>
								{line || " "}
							</span>
						)}
					</div>
				);
			})}
		</pre>
	);
}

function hunkBody(lines: { line: string }[], start: number) {
	const result: string[] = [];
	for (let i = start + 1; i < lines.length; i++) {
		const line = lines[i]?.line ?? "";
		if (line.startsWith("@@")) break;
		if (line.startsWith("+") || line.startsWith(" ")) result.push(line.slice(1));
	}
	return result;
}

function numberedLines(text: string) {
	let next: number | undefined;
	return text.split("\n").map((line, number) => {
		if (line.startsWith("@@")) {
			const hunk = /^@@ -\d+(?:,\d+)? \+(\d+)/.exec(line);
			next = hunk ? Number(hunk[1]) : undefined;
		}
		const position =
			next !== undefined && (line.startsWith(" ") || line.startsWith("+")) ? next++ : undefined;
		return { line, number, position };
	});
}
