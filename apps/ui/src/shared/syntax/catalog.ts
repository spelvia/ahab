/**
 * The languages plugins contribute, kept apart from the
 * bundled ones so the app's first load carries no language: the bundled
 * list and the engine load with the first thing to highlight
 * (`client.ts`), and read these.
 */

import type { Language } from "@/shared/syntax/model";

let contributed: { source: string; languages: Language[] }[] = [];
const listeners = new Set<() => void>();

function changed() {
	for (const listener of listeners) listener();
}

/**
 * Adds a plugin's languages (its manifest's `contributes.languages`);
 * returns a way to remove them. A language with a bundled language's ID
 * replaces it; the plugin enabled last wins where file patterns overlap.
 */
export function contributeLanguages(source: string, languages: Language[]): () => void {
	const entry = { source, languages };
	contributed = [...contributed.filter((item) => item.source !== source), entry];
	changed();
	return () => {
		if (!contributed.includes(entry)) return;
		contributed = contributed.filter((item) => item !== entry);
		changed();
	};
}

/** Every contributed language, in the order their plugins came. */
export function contributedLanguages(): Language[] {
	return contributed.flatMap((item) => item.languages);
}

export function subscribeLanguages(listener: () => void): () => void {
	listeners.add(listener);
	return () => listeners.delete(listener);
}
