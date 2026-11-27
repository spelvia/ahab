/**
 * Where highlighting and language tooling plug into
 * every editor: register once, at startup or when a plugin loads; editors
 * opened afterwards ask the registered ones, newest first. The app
 * registers the Tree-sitter engine (`shared/syntax`) at startup.
 */

import type {
	ExternalFile,
	EditorDocument,
	EmitFolds,
	EmitTokens,
	HighlightSession,
	Highlighter,
	LanguageFile,
	LanguageProvider,
	LanguageService,
} from "@/shared/editor/model";
import { t } from "@/shared/i18n";

const highlighters: Highlighter[] = [];
const languages: LanguageProvider[] = [];

/** @public the extension point: adds a highlighting engine; returns a way to remove it. */
export function registerHighlighter(highlighter: Highlighter): () => void {
	highlighters.unshift(highlighter);
	return () => {
		const at = highlighters.indexOf(highlighter);
		if (at !== -1) highlighters.splice(at, 1);
	};
}

/** @public the extension point: adds a source of language tooling; returns a way to remove it. */
export function registerLanguageService(provider: LanguageProvider): () => void {
	languages.unshift(provider);
	return () => {
		const at = languages.indexOf(provider);
		if (at !== -1) languages.splice(at, 1);
	};
}

/** The first engine that highlights `document`. */
export function openHighlighting(
	document: EditorDocument,
	emit: EmitTokens,
	folds?: EmitFolds,
): HighlightSession | null {
	for (const highlighter of highlighters) {
		const session = highlighter.open(document, emit, folds);
		if (session) return session;
	}
	return null;
}

/** The first tooling for a file. */
export function openLanguageService(file: LanguageFile): LanguageService | null {
	for (const provider of languages) {
		const service = provider(file);
		if (service) return service;
	}
	return null;
}

type ExternalLoader = (projectId: string, path: string) => Promise<ExternalFile>;
let externalLoader: ExternalLoader | null = null;

/** @public the extension point: reads files outside the project (`external:<token>`). */
export function registerExternalFiles(loader: ExternalLoader): () => void {
	externalLoader = loader;
	return () => {
		if (externalLoader === loader) externalLoader = null;
	};
}

/** Whether a path names a file outside the project. */
export function isExternal(path: string): boolean {
	return path.startsWith("external:");
}

/** A file outside the project, through the registered loader. */
export function loadExternalFile(projectId: string, path: string): Promise<ExternalFile> {
	if (!externalLoader) return Promise.reject(new Error(t("editor.noExternalFiles")));
	return externalLoader(projectId, path);
}

const externalNames = new Map<string, string>();

/** Remembers an external file's name, for its tab. */
export function nameExternal(path: string, name: string): void {
	externalNames.set(path, name);
}

/** An external file's name, when known. */
export function externalName(path: string): string | undefined {
	return externalNames.get(path);
}
