import { useCallback, useState } from "react";

/** A one-time code this tab created, kept until it is used or expires. */
export interface StoredCode {
	id: string;
	code: string;
	expiresAt: string;
}

const KEY = "ahab.join-code";

/** The code this tab keeps, when it still works. */
export function readStoredCode(now: Date = new Date()): StoredCode | undefined {
	try {
		const text = sessionStorage.getItem(KEY);
		if (!text) return undefined;
		const stored = JSON.parse(text) as StoredCode;
		if (new Date(stored.expiresAt) <= now) {
			sessionStorage.removeItem(KEY);
			return undefined;
		}
		return stored;
	} catch {
		return undefined;
	}
}

/**
 * The one-time code shown in this tab. A code is shown only once by the
 * server, so the tab keeps it (sessionStorage: this tab only, gone with it)
 * until it is used, withdrawn or expires; a refresh shows the same code.
 */
export function useStoredCode() {
	const [stored, setStored] = useState(() => readStoredCode());
	const keep = useCallback((code: StoredCode) => {
		sessionStorage.setItem(KEY, JSON.stringify(code));
		setStored(code);
	}, []);
	const forget = useCallback(() => {
		sessionStorage.removeItem(KEY);
		setStored(undefined);
	}, []);
	return { stored, keep, forget };
}
