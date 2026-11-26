/** Ahab's documentation site (`apps/docs`). */
const DOCS_URL = "https://useahab.com/docs";

/** A page of the documentation, such as `/installation/cli/`. */
export function docs(path: string) {
	return `${DOCS_URL}${path}`;
}
