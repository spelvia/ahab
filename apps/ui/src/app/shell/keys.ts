/**
 * The workbench's keys, read from the keymap (`@/shared/keymap`): each
 * shortcut asks for an action, and the workbench says what each does. An
 * action the workbench leaves out (its core feature is off) has no keys.
 */

import { useCallback, useEffect, useRef } from "react";

import { type ActionId, actionFor, hintFor } from "@/shared/keymap/model";
import { useKeymap } from "@/shared/keymap/use-keymap";
import { appWindow } from "@/shared/platform";

/** What each action does here; those left out do nothing. */
export type WorkbenchKeys = Partial<Record<ActionId, () => void>>;

export function useWorkbenchKeys(keys: WorkbenchKeys) {
	const map = useKeymap();
	// The newest handlers, without listening again on every render.
	const current = useRef(keys);
	useEffect(() => {
		current.current = keys;
	});
	useEffect(() => {
		const onKey = (event: KeyboardEvent) => {
			const action = actionFor(map, event, (each) => Boolean(current.current[each.id]));
			if (!action) return;
			event.preventDefault();
			current.current[action.id]?.();
		};
		window.addEventListener("keydown", onKey);
		return () => window.removeEventListener("keydown", onKey);
	}, [map]);
}

/** How to show each action's key here: a function of the action, kept up to date. */
export function useHints(): (id: ActionId) => string | undefined {
	const map = useKeymap();
	return useCallback((id: ActionId) => hintFor(map, id, appWindow), [map]);
}
