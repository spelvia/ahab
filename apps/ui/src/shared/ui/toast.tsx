import { useSyncExternalStore } from "react";

import { app } from "@/shared/desktop/bridge";

interface ToastAction {
	label: string;
	run: () => void;
}

interface Toast {
	id: number;
	message: string;
	/** A second, quieter line: where it happened. */
	detail?: string;
	actions: ToastAction[];
}

let toasts: Toast[] = [];
let next = 1;
const listeners = new Set<() => void>();

function set(list: Toast[]) {
	toasts = list;
	for (const listener of listeners) listener();
}

function dismiss(id: number) {
	set(toasts.filter((item) => item.id !== id));
}

/** Shows a short message at the bottom, with optional actions such as Undo. */
export function toast(
	message: string,
	options: {
		action?: ToastAction;
		actions?: ToastAction[];
		duration?: number;
		detail?: string;
		/** In the desktop app, shown as the system's notification while in the background (default yes). */
		native?: boolean;
	} = {},
) {
	const id = next++;
	const actions = [...(options.action ? [options.action] : []), ...(options.actions ?? [])];
	// One at a time: a new message replaces the last.
	set([{ id, message, detail: options.detail, actions }]);
	// In the desktop app, a window in the background shows it as a native
	// notification too, which opens this page.
	if (options.native !== false && typeof document !== "undefined" && !document.hasFocus())
		void app.notify({
			title: message,
			body: options.detail,
			target: globalThis.location?.pathname,
		});
	setTimeout(() => dismiss(id), options.duration ?? 4000);
}

/** Shows what went wrong, long enough to read it. */
export function toastError(error: Error) {
	toast(error.message, { duration: 8000 });
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

/** Where toasts appear; mounted once, in the root layout. */
export function Toaster() {
	const list = useSyncExternalStore(
		subscribe,
		() => toasts,
		() => toasts,
	);
	return (
		<div
			aria-live="polite"
			className="pointer-events-none fixed inset-x-0 bottom-4 z-[60] flex flex-col items-center gap-2 px-4"
		>
			{list.map((item) => (
				<div
					key={item.id}
					role="status"
					className="bg-popover text-popover-foreground border-border animate-in fade-in-0 slide-in-from-bottom-2 pointer-events-auto flex items-center gap-3 rounded-lg border py-2 pr-2 pl-3.5 text-sm shadow-[0_8px_24px_-8px_rgb(0_0_0/0.45)]"
				>
					<span className="min-w-0">
						<span className="block">{item.message}</span>
						{item.detail ? (
							<span className="text-muted-foreground block truncate text-xs">{item.detail}</span>
						) : null}
					</span>
					{item.actions.map((action) => (
						<button
							key={action.label}
							type="button"
							onClick={() => {
								dismiss(item.id);
								action.run();
							}}
							className="text-primary hover:bg-accent focus-visible:ring-ring rounded-md px-2 py-1 font-semibold outline-none focus-visible:ring-2"
						>
							{action.label}
						</button>
					))}
				</div>
			))}
		</div>
	);
}
