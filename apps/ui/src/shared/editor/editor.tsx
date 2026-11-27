/**
 * The editor for file windows, loaded the first time a window
 * needs it: CodeMirror and its parts stay out of the app's first load.
 */

import { forwardRef, lazy, Suspense } from "react";

import type { CodeEditorProps, EditorHandle } from "@/shared/editor/code-editor";
import { useTranslation } from "@/shared/i18n";

export type { EditorHandle, SelectionReport } from "@/shared/editor/code-editor";

const CodeEditor = lazy(() =>
	import("@/shared/editor/code-editor").then((module) => ({ default: module.CodeEditor })),
);
const Compare = lazy(() =>
	import("@/shared/editor/compare").then((module) => ({ default: module.Compare })),
);

/** A file's text in the editor: read-only, or editable. */
export const Editor = forwardRef<EditorHandle, CodeEditorProps>(function Editor(props, ref) {
	const { t } = useTranslation();
	return (
		<Suspense fallback={<p className="text-muted-foreground p-4 text-sm">{t("editor.loading")}</p>}>
			<CodeEditor ref={ref} {...props} />
		</Suspense>
	);
});

/** Two versions of a file side by side, their differences marked. */
export function CompareTexts(props: {
	left: string;
	right: string;
	leftLabel: string;
	rightLabel: string;
}) {
	const { t } = useTranslation();
	return (
		<Suspense fallback={<p className="text-muted-foreground p-4 text-sm">{t("state.loading")}</p>}>
			<Compare {...props} />
		</Suspense>
	);
}
