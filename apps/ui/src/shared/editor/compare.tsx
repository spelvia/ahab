/**
 * Two versions of a file side by side (CodeMirror's merge view), for a
 * save that met a change on disk: what is on disk, and yours.
 * Loaded only when asked for.
 */

import { MergeView } from "@codemirror/merge";
import { EditorState } from "@codemirror/state";
import { EditorView, lineNumbers } from "@codemirror/view";
import { useEffect, useRef } from "react";

const look = EditorView.theme({
	"&": { backgroundColor: "var(--background)", color: "var(--foreground)", fontSize: "12.5px" },
	".cm-scroller": { fontFamily: "var(--font-mono, ui-monospace, monospace)" },
	".cm-gutters": { backgroundColor: "var(--background)", border: "none" },
});

export function Compare({
	left,
	right,
	leftLabel,
	rightLabel,
}: {
	left: string;
	right: string;
	leftLabel: string;
	rightLabel: string;
}) {
	const box = useRef<HTMLDivElement>(null);
	useEffect(() => {
		if (!box.current) return;
		const side = [lineNumbers(), EditorState.readOnly.of(true), look];
		const view = new MergeView({
			a: { doc: left, extensions: side },
			b: { doc: right, extensions: side },
			parent: box.current,
			collapseUnchanged: { margin: 3, minSize: 6 },
		});
		return () => view.destroy();
	}, [left, right]);
	return (
		<div className="flex min-h-0 flex-1 flex-col">
			<div className="text-muted-foreground border-border grid shrink-0 grid-cols-2 border-b text-xs">
				<span className="px-4 py-1">{leftLabel}</span>
				<span className="border-border border-l px-4 py-1">{rightLabel}</span>
			</div>
			<div ref={box} className="min-h-0 flex-1 overflow-auto" />
		</div>
	);
}
