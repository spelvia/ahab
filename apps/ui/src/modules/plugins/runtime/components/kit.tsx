/**
 * What the web app shares with plugins' web entries: React, zod,
 * the SDK's definitions, and `@ahab/plugin/web` — Ahab's components and
 * hooks. Put on `globalThis.__ahabShared` before any plugin loads; the
 * files in `/plugin-runtime/` hand them to plugins.
 */
import * as shared from "@ahab/plugin/shared";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { cn } from "cn";
import * as React from "react";
import * as ReactDOM from "react-dom";
import * as ReactDOMClient from "react-dom/client";
import * as jsxRuntime from "react/jsx-runtime";
import * as zod from "zod";

import { usePluginScope } from "@/modules/plugins/runtime/components/scope";
import { useHost } from "@/modules/plugins/runtime/hooks";
import { serverSettings, useEffectiveSettings, useSaveSettings } from "@/modules/plugins/setting";
import { TerminalView, useTerminal } from "@/modules/sessions";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuTrigger,
} from "@/shared/ui/context-menu";
import { CopyButton } from "@/shared/ui/copy-button";
import {
	Dialog,
	DialogClose,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
	DialogTrigger,
} from "@/shared/ui/dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { FileIcon } from "@/shared/ui/file-icon";
import { IconButton } from "@/shared/ui/icon-button";
import { Markdown } from "@/shared/ui/markdown";
import { NameDialog } from "@/shared/ui/name-dialog";
import { Note } from "@/shared/ui/note";
import { toast, toastError } from "@/shared/ui/toast";

/**
 * Dialogs and menus render outside the plugin's content (portalled to the
 * page's end), so they carry the plugin's mark themselves: its classes
 * still apply inside them.
 */
function inScope<P extends object>(Component: React.ComponentType<P>) {
	function InScope(props: P) {
		const plugin = usePluginScope();
		return <Component {...props} data-ahab-plugin={plugin} />;
	}
	InScope.displayName = `InScope(${Component.displayName ?? Component.name})`;
	return InScope;
}

const ScopedDialogContent = inScope(DialogContent);
const ScopedDropdownMenuContent = inScope(DropdownMenuContent);
const ScopedContextMenuContent = inScope(ContextMenuContent);

/** A terminal Ahab runs, by its ID, as in Ahab's own windows. */
function Terminal({ id }: { id: string }) {
	const { t } = useTranslation("plugins");
	const terminal = useTerminal(id);
	if (terminal.isError)
		return <p className="text-muted-foreground p-4 text-sm">{t("runtime.terminalGone")}</p>;
	if (!terminal.data) return null;
	return <TerminalView terminal={terminal.data} />;
}

function useProject() {
	return useHost().project;
}

/**
 * A plugin's settings in effect for the current project, and a way to
 * change them everywhere (the server's values: the fields given replace
 * those there, the rest stay).
 */
function useSettings(plugin: string) {
	const project = useHost().project;
	const effective = useEffectiveSettings(plugin, project?.id);
	const save = useSaveSettings();
	return {
		values: effective.data?.values as Record<string, unknown> | undefined,
		save: async (values: Record<string, unknown>) => {
			const current = (await serverSettings(plugin)).values as Record<string, unknown>;
			await save.mutateAsync({
				id: plugin,
				params: { scope: "server" },
				data: { values: { ...current, ...values } },
			});
		},
	};
}

/** `@ahab/plugin/web` as Ahab provides it. */
export const webRuntime = {
	...shared,
	definePlugin: <T,>(setup: T) => setup,
	Button,
	IconButton,
	CopyButton,
	Dialog,
	DialogTrigger,
	DialogClose,
	DialogContent: ScopedDialogContent,
	DialogHeader,
	DialogBody,
	DialogFooter,
	DialogTitle,
	DialogDescription,
	DropdownMenu,
	DropdownMenuTrigger,
	DropdownMenuContent: ScopedDropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	ContextMenu,
	ContextMenuTrigger,
	ContextMenuContent: ScopedContextMenuContent,
	ContextMenuItem,
	ConfirmDialog,
	NameDialog,
	Note,
	Markdown,
	FileIcon,
	Terminal,
	toast,
	toastError,
	cn,
	useProject,
	useSettings,
	useQuery,
	useQueryClient,
};

/** Shares the modules with plugins; once is enough. */
export function shareWithPlugins() {
	// The name the generated `/plugin-runtime/` files read.
	// oxlint-disable-next-line no-underscore-dangle
	(globalThis as { __ahabShared?: unknown }).__ahabShared = {
		react: React,
		"react/jsx-runtime": jsxRuntime,
		"react-dom": ReactDOM,
		"react-dom/client": ReactDOMClient,
		zod,
		"@ahab/plugin/shared": shared,
		"@ahab/plugin/web": webRuntime,
	};
}
