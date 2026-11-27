/**
 * Copies `text`, and says whether it worked. The Clipboard API exists only in
 * secure contexts; a server reached over plain http (which Ahab supports)
 * gets the older selection-based copy instead.
 */
export async function copyText(text: string): Promise<boolean> {
	try {
		if (navigator.clipboard) {
			await navigator.clipboard.writeText(text);
			return true;
		}
	} catch {
		// Fall through to the older way.
	}
	const field = document.createElement("textarea");
	field.value = text;
	field.setAttribute("readonly", "");
	field.style.position = "fixed";
	field.style.opacity = "0";
	document.body.append(field);
	field.select();
	try {
		return document.execCommand("copy");
	} catch {
		return false;
	} finally {
		field.remove();
	}
}
