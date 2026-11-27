/** Whether this is a Mac (or iPad): ⌘ where others have Ctrl. */
export const mac =
	typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform ?? "");

/** How the command key is shown: "⌘" on a Mac, "Ctrl+" elsewhere. */
export const commandKey = mac ? "⌘" : "Ctrl+";

/** Whether Ahab runs installed, in a window of its own rather than a browser tab. */
export const appWindow =
	typeof window !== "undefined" &&
	["standalone", "window-controls-overlay", "fullscreen"].some(
		(mode) => window.matchMedia?.(`(display-mode: ${mode})`).matches,
	);
