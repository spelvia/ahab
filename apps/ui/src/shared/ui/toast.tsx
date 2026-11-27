import { cn } from "cn";
import { CircleAlert, CircleCheck, Info, TriangleAlert, X } from "lucide-react";
import { useSyncExternalStore } from "react";

import { app } from "@/shared/desktop/bridge";
import { useTranslation } from "@/shared/i18n";

interface ToastAction {
	label: string;
	run: () => void;
}

/** What a toast is about: its icon and colour. Plain when absent. */
export type ToastTone = "info" | "success" | "warning" | "error";

interface Toast {
	id: number;
	message: string;
	/** A line under the message, as readable as it: what it is about. */
	body?: string;
	/** A quieter last line: where it happened. */
	detail?: string;
	tone?: ToastTone;
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

/**
 * Shows a short message, with optional actions such as Undo: at the top on
 * a phone (clear of the composer), at the bottom right on wider screens.
 */
export function toast(
	message: string,
	options: {
		action?: ToastAction;
		actions?: ToastAction[];
		duration?: number;
		body?: string;
		detail?: string;
		tone?: ToastTone;
		/** In the desktop app, shown as the system's notification while in the background (default yes). */
		native?: boolean;
	} = {},
) {
	const id = next++;
	const actions = [...(options.action ? [options.action] : []), ...(options.actions ?? [])];
	// One at a time: a new message replaces the last.
	set([{ id, message, body: options.body, detail: options.detail, tone: options.tone, actions }]);
	// In the desktop app, a window in the background shows it as a native
	// notification too, which opens this page.
	if (options.native !== false && typeof document !== "undefined" && !document.hasFocus())
		void app.notify({
			title: message,
			body: [options.body, options.detail].filter(Boolean).join("\n") || undefined,
			target: globalThis.location?.pathname,
		});
	setTimeout(() => dismiss(id), options.duration ?? 4000);
}

/** Shows what went wrong, long enough to read it. */
export function toastError(error: Error) {
	toast(error.message, { duration: 8000, tone: "error" });
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

const TONES = {
	info: { Icon: Info, className: "text-muted-foreground" },
	success: { Icon: CircleCheck, className: "text-emerald-500" },
	warning: { Icon: TriangleAlert, className: "text-amber-500" },
	error: { Icon: CircleAlert, className: "text-destructive" },
} as const;

/** Where toasts appear; mounted once, in the root layout. */
export function Toaster() {
	const { t } = useTranslation("common");
	const list = useSyncExternalStore(
		subscribe,
		() => toasts,
		() => toasts,
	);
	return (
		<div
			aria-live="polite"
			className={cn(
				"pointer-events-none fixed inset-x-0 z-[60] flex flex-col items-center gap-2 px-3",
				// A phone: under the header, so the composer stays free.
				"top-[calc(env(safe-area-inset-top)+3.5rem)]",
				"sm:top-auto sm:right-4 sm:bottom-4 sm:left-auto sm:items-end sm:px-0",
			)}
		>
			{list.map((item) => {
				const tone = item.tone ? TONES[item.tone] : undefined;
				return (
					<div
						key={item.id}
						role="status"
						className={cn(
							"bg-popover text-popover-foreground border-border pointer-events-auto flex w-full max-w-sm items-start gap-3 rounded-xl border p-3 text-sm shadow-[0_12px_32px_-12px_rgb(0_0_0/0.5)]",
							"animate-in fade-in-0 slide-in-from-top-2 sm:slide-in-from-bottom-2",
						)}
					>
						{tone ? (
							<tone.Icon aria-hidden className={cn("mt-0.5 size-4 shrink-0", tone.className)} />
						) : null}
						<span className="min-w-0 flex-1">
							<span className={cn("block", (item.body || item.detail) && "font-medium")}>
								{item.message}
							</span>
							{item.body ? <span className="mt-0.5 block truncate">{item.body}</span> : null}
							{item.detail ? (
								<span className="text-muted-foreground mt-0.5 block truncate text-xs">
									{item.detail}
								</span>
							) : null}
							{item.actions.length ? (
								<span className="mt-2 flex gap-1">
									{item.actions.map((action) => (
										<button
											key={action.label}
											type="button"
											onClick={() => {
												dismiss(item.id);
												action.run();
											}}
											className="border-border hover:bg-accent focus-visible:ring-ring rounded-md border px-2.5 py-1 text-xs font-medium outline-none focus-visible:ring-2"
										>
											{action.label}
										</button>
									))}
								</span>
							) : null}
						</span>
						<button
							type="button"
							aria-label={t("actions.close")}
							onClick={() => dismiss(item.id)}
							className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring -mt-1 -mr-1 rounded-md p-1 outline-none focus-visible:ring-2"
						>
							<X aria-hidden className="size-3.5" />
						</button>
					</div>
				);
			})}
		</div>
	);
}
