/**
 * A web entry's SDK. In Ahab this module is Ahab's own: a plugin's build
 * points `@ahab/plugin/web` at `/plugin-runtime/ahab-plugin-web.js`, which
 * hands it what the web app provides. This file gives the types (and
 * stand-ins that throw if used outside Ahab).
 */
import type { ButtonHTMLAttributes, ComponentType, HTMLAttributes, ReactNode } from "react";

export * from "../shared/index.ts";

type Remove = () => void;

/** Where a machine command or channel runs. */
export interface Target {
	/** A machine's ID. */
	machine?: string;
	/** A project's ID: its machine, and its folder for commands. */
	project?: string;
}

/** A command or channel: its full name (`docker.containers.list`), or one
 * of this plugin's definitions. */
export type Ref = string | { name: string };

/** A stream with the plugin's daemon entry. */
export interface WebChannel {
	write(data: Uint8Array | string): void;
	onData(listener: (data: Uint8Array) => void): Remove;
	/** Called once when it ends: with the plugin's reason, if it gave one. */
	onClose(listener: (reason?: string) => void): Remove;
	close(): void;
}

/** The project the person is in. */
export interface Project {
	id: string;
	machineId: string;
	name: string;
	path: string;
}

/** What a web entry's function gets. */
export interface WebContext {
	readonly plugin: string;
	commands: {
		/** Runs a command; where `target` says, else wherever its plugin runs. */
		call<Output = unknown>(command: Ref, input?: unknown, target?: Target): Promise<Output>;
	};
	channels: {
		open(channel: Ref, params?: unknown, target?: Target): WebChannel;
	};
	events: {
		/** Ahab's events (`terminal.exited`) and plugins' (`docker.container.changed`). */
		on(name: string, listener: (data: unknown) => void): Remove;
	};
	/** A sidebar view, beside Sessions, Files, Git and Search. */
	views: {
		add(view: { id: string; title: string; icon: ReactNode; Component: ComponentType }): Remove;
	};
	/** An activity bar icon that opens (or brings forward) one of the plugin's windows. */
	activity: {
		add(item: {
			id: string;
			title: string;
			icon: ReactNode;
			opens: { window: string; params?: unknown };
		}): Remove;
	};
	/** Kinds of window in the tiles, beside sessions and files. */
	windows: {
		add<P>(window: {
			kind: string;
			title: (params: P) => string;
			/** One per kind: opening it again brings it forward with the new params. */
			single?: boolean;
			Component: ComponentType<{ params: P }>;
		}): Remove;
		open<P>(kind: string, params?: P): void;
	};
	/** Entries in the + menus. */
	newWindow: {
		add(item: { id: string; title: string; icon?: ReactNode; run: () => void }): Remove;
	};
	/** Commands in the palette. */
	palette: {
		add(item: { id: string; title: string; keywords?: string[]; run: () => void }): Remove;
	};
	/** Small items in the status bar, after Ahab's own on the left. */
	status: {
		add(item: { id: string; Component: ComponentType }): Remove;
	};
	/** The plugin's own settings page, instead of the generated one. */
	settings: {
		page(page: { Component: ComponentType }): Remove;
	};
	/** Terminals in Ahab, opened in a window; the plugin's hooks may change them. */
	terminals: {
		start(options: {
			/** Else the current project. */
			project?: string;
			title?: string;
			command?: string;
			/** What it is for, by plugin: `{ docker: { container: "web" } }`. */
			context?: Record<string, unknown>;
			/** Open it in a window (the default); false to show it yourself (`<Terminal id>`). */
			show?: boolean;
		}): Promise<{ session: string; terminal: string }>;
	};
	/** The current project, if any. */
	project(): Project | undefined;
}

/** A web entry: `export default definePlugin((ahab) => { … })`. */
export function definePlugin(
	setup: (ahab: WebContext) => void | Remove,
): (ahab: WebContext) => void | Remove {
	return setup;
}

function outside(name: string): never {
	throw new Error(`${name} is Ahab's: it works only inside Ahab's web app.`);
}

type Props = HTMLAttributes<HTMLElement> & { children?: ReactNode; className?: string };

/** Ahab's button. */
export const Button: ComponentType<
	ButtonHTMLAttributes<HTMLButtonElement> & {
		variant?: "default" | "outline" | "ghost" | "destructive" | "secondary" | "link";
		size?: "default" | "sm" | "lg" | "icon";
		asChild?: boolean;
	}
> = () => outside("Button");
/** A small square button with an icon and a label for screen readers. */
export const IconButton: ComponentType<{
	label: string;
	onClick: () => void;
	disabled?: boolean;
	pressed?: boolean;
	className?: string;
	children: ReactNode;
}> = () => outside("IconButton");
export const CopyButton: ComponentType<{ text: string; label: string }> = () =>
	outside("CopyButton");
export const Dialog: ComponentType<{
	open?: boolean;
	onOpenChange?: (open: boolean) => void;
	children?: ReactNode;
}> = () => outside("Dialog");
export const DialogTrigger: ComponentType<Props & { asChild?: boolean }> = () =>
	outside("DialogTrigger");
export const DialogClose: ComponentType<Props & { asChild?: boolean }> = () =>
	outside("DialogClose");
/** `size`: "sm", "md" (the default), "lg" or "xl". Lay it out with `DialogHeader`, `DialogBody` and `DialogFooter`. */
export const DialogContent: ComponentType<Props & { size?: "sm" | "md" | "lg" | "xl" }> = () =>
	outside("DialogContent");
export const DialogHeader: ComponentType<Props> = () => outside("DialogHeader");
export const DialogBody: ComponentType<Props> = () => outside("DialogBody");
export const DialogFooter: ComponentType<Props> = () => outside("DialogFooter");
export const DialogTitle: ComponentType<Props> = () => outside("DialogTitle");
export const DialogDescription: ComponentType<Props> = () => outside("DialogDescription");
export const DropdownMenu: ComponentType<{
	open?: boolean;
	onOpenChange?: (open: boolean) => void;
	children?: ReactNode;
}> = () => outside("DropdownMenu");
export const DropdownMenuTrigger: ComponentType<Props & { asChild?: boolean }> = () =>
	outside("DropdownMenuTrigger");
export const DropdownMenuContent: ComponentType<
	Props & { align?: "start" | "center" | "end" }
> = () => outside("DropdownMenuContent");
export const DropdownMenuItem: ComponentType<
	Props & { onSelect?: () => void; disabled?: boolean }
> = () => outside("DropdownMenuItem");
export const DropdownMenuSeparator: ComponentType<Props> = () => outside("DropdownMenuSeparator");
export const ContextMenu: ComponentType<{ children?: ReactNode }> = () => outside("ContextMenu");
export const ContextMenuTrigger: ComponentType<Props & { asChild?: boolean }> = () =>
	outside("ContextMenuTrigger");
export const ContextMenuContent: ComponentType<Props> = () => outside("ContextMenuContent");
export const ContextMenuItem: ComponentType<
	Props & { onSelect?: () => void; disabled?: boolean }
> = () => outside("ContextMenuItem");
/** Asks before something that cannot be undone. */
export const ConfirmDialog: ComponentType<{
	open: boolean;
	onOpenChange: (open: boolean) => void;
	title: string;
	description?: ReactNode;
	action: string;
	destructive?: boolean;
	pending?: boolean;
	error?: string;
	onConfirm: () => void;
}> = () => outside("ConfirmDialog");
/** Asks for one line of text. */
export const NameDialog: ComponentType<{
	open: boolean;
	onOpenChange: (open: boolean) => void;
	title: string;
	label: string;
	action: string;
	initial?: string;
	pending?: boolean;
	error?: string;
	onSubmit: (value: string) => void;
}> = () => outside("NameDialog");
/** A note: a sentence beside an icon, in a quiet box. */
export const Note: ComponentType<{ className?: string; children: ReactNode }> = () =>
	outside("Note");
/** Markdown, as Ahab shows it in chats. */
export const Markdown: ComponentType<{ text: string; className?: string }> = () =>
	outside("Markdown");
/** A file's icon, from the icon theme, by its path or name. */
export const FileIcon: ComponentType<{ path: string; className?: string }> = () =>
	outside("FileIcon");
/** A terminal Ahab runs (by its ID), shown as in Ahab's terminal windows. */
export const Terminal: ComponentType<{ id: string }> = () => outside("Terminal");
/** A short message in the corner. */
export const toast: (message: string, options?: { duration?: number }) => void = () =>
	outside("toast");
/** An error's message in the corner, for longer. */
export const toastError: (error: Error) => void = () => outside("toastError");
/** Joins class names, as Ahab's components do (Tailwind classes merge). */
export const cn: (...classes: unknown[]) => string = () => outside("cn");
/** The current project, kept up to date. */
export const useProject: () => Project | undefined = () => outside("useProject");
/** The plugin's settings in effect here, and a way to change them. */
export const useSettings: <T = Record<string, unknown>>(
	plugin: string,
) => { values: T | undefined; save: (values: T) => Promise<void> } = () => outside("useSettings");
/** TanStack Query, sharing Ahab's client. */
export const useQuery: (options: {
	queryKey: readonly unknown[];
	queryFn: () => Promise<unknown>;
	refetchInterval?: number | false;
	enabled?: boolean;
	staleTime?: number;
}) => {
	data: unknown;
	error: unknown;
	isPending: boolean;
	isFetching: boolean;
	refetch: () => Promise<{ data: unknown }>;
} = () => outside("useQuery");
export const useQueryClient: () => {
	invalidateQueries: (filters: { queryKey: readonly unknown[] }) => Promise<void>;
} = () => outside("useQueryClient");

/** Every name Ahab's `@ahab/plugin/web` provides; Ahab checks it gives them all. */
export const WEB_EXPORTS = [
	"definePlugin",
	"Button",
	"IconButton",
	"CopyButton",
	"Dialog",
	"DialogTrigger",
	"DialogClose",
	"DialogContent",
	"DialogHeader",
	"DialogBody",
	"DialogFooter",
	"DialogTitle",
	"DialogDescription",
	"DropdownMenu",
	"DropdownMenuTrigger",
	"DropdownMenuContent",
	"DropdownMenuItem",
	"DropdownMenuSeparator",
	"ContextMenu",
	"ContextMenuTrigger",
	"ContextMenuContent",
	"ContextMenuItem",
	"ConfirmDialog",
	"NameDialog",
	"Note",
	"Markdown",
	"FileIcon",
	"Terminal",
	"toast",
	"toastError",
	"cn",
	"useProject",
	"useSettings",
	"useQuery",
	"useQueryClient",
	"defineCommand",
	"defineEvent",
	"defineChannel",
	"defineSettings",
	"jsonSchema",
	"z",
] as const;
