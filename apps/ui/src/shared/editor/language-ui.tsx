/**
 * What a file window shows of its language tooling, in the
 * editor's terms: the status item and its popover, renaming with a preview,
 * and the fixes offered at the cursor.
 */
import { useNavigate } from "@tanstack/react-router";
import { cn } from "cn";
import { Braces, Check, CircleAlert, Loader2, X } from "lucide-react";
import { useState } from "react";

import type {
	CodeActionRequest,
	FileEdits,
	LanguageStatus,
	RenameRequest,
} from "@/shared/editor/model";
import { i18n, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { toast, toastError } from "@/shared/ui/toast";

/** What the status bar says after `{}`: the label without the language (`✓`, `starting…`). */
function languageState(status: LanguageStatus): string {
	const rest = status.label.startsWith(status.title)
		? status.label.slice(status.title.length)
		: status.label;
	return rest.replace(/^\s*·?\s*/, "");
}

/**
 * `Rust ✓`, `Rust · starting…`, `Java · basic`; a popover with its tools.
 * In the status bar (`triggerClassName`), `{} ✓`: the language shows beside it.
 */
export function LanguageStatusItem({
	status,
	triggerClassName,
}: {
	status: LanguageStatus;
	triggerClassName?: string;
}) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	const [open, setOpen] = useState(false);
	const state = languageState(status);
	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger
				className={cn(
					triggerClassName ??
						"hover:bg-accent/60 focus-visible:ring-ring flex h-5 items-center gap-1 rounded px-1.5 outline-none focus-visible:ring-2",
					status.tone === "warning"
						? "text-warning hover:text-warning"
						: triggerClassName
							? undefined
							: "text-muted-foreground hover:text-foreground",
				)}
				aria-label={t("editor.language.tools", { label: status.label })}
				title={triggerClassName ? t("editor.language.tools", { label: status.label }) : undefined}
			>
				{triggerClassName ? <Braces className="size-3.5" /> : null}
				{status.tone === "busy" ? <Loader2 className="size-3 animate-spin" /> : null}
				{triggerClassName ? state : status.label}
			</PopoverTrigger>
			<PopoverContent align="end" className="w-96 p-3 text-xs">
				<div className="mb-2 flex items-baseline gap-2">
					<span className="font-medium">{status.title}</span>
					{status.root ? (
						<span className="text-muted-foreground ml-auto font-mono">{status.root}</span>
					) : null}
				</div>
				{status.rows.length ? (
					<ul className="space-y-1">
						{status.rows.map((row) => (
							<li key={row.name} className="flex items-start gap-2">
								{row.ok === null ? (
									<Loader2 className="mt-0.5 size-3 shrink-0 animate-spin" />
								) : row.ok ? (
									<Check className="text-success mt-0.5 size-3 shrink-0" />
								) : (
									<X className="text-destructive mt-0.5 size-3 shrink-0" />
								)}
								<span className="shrink-0 font-medium">{row.name}</span>
								<span className="text-muted-foreground min-w-0">{row.detail}</span>
							</li>
						))}
					</ul>
				) : (
					<p className="text-muted-foreground">{t("editor.language.nothingConfigured")}</p>
				)}
				{status.notes.map((note) => (
					<p key={note} className="text-muted-foreground mt-2">
						{note}
					</p>
				))}
				{status.actions.length ? (
					<div className="mt-3 flex flex-wrap gap-1.5">
						{status.actions.map((action) => (
							<Button
								key={action.label}
								size="sm"
								variant="outline"
								className="h-6 px-2 text-xs"
								onClick={() => {
									setOpen(false);
									if (action.run) action.run();
									else if (action.to) void navigate({ to: action.to });
								}}
							>
								{action.label}
							</Button>
						))}
					</div>
				) : null}
			</PopoverContent>
		</Popover>
	);
}

/** How many edits, in how many files. */
function count(edits: FileEdits[]): string {
	const total = edits.reduce((sum, file) => sum + file.edits.length, 0);
	return i18n.t("editor.rename.changes", {
		count: total,
		files: i18n.t("editor.rename.files", { count: edits.length }),
	});
}

/** F2: the new name, then the files it changes, then applying it. */
export function RenameDialog({
	request,
	onClose,
}: {
	request: RenameRequest;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const [name, setName] = useState(request.name);
	const [preview, setPreview] = useState<FileEdits[] | null>(null);
	const [busy, setBusy] = useState(false);
	const ask = async () => {
		setBusy(true);
		try {
			setPreview(await request.rename(name));
		} catch (error) {
			toastError(error instanceof Error ? error : new Error(String(error)));
		} finally {
			setBusy(false);
		}
	};
	const apply = async () => {
		if (!preview) return;
		setBusy(true);
		try {
			await request.apply(preview);
			toast(t("editor.rename.done", { summary: count(preview) }));
			onClose();
		} catch (error) {
			toastError(error instanceof Error ? error : new Error(String(error)));
			setBusy(false);
		}
	};
	return (
		<Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
			<DialogContent size="md">
				<DialogHeader>
					<DialogTitle>{t("editor.rename.title", { name: request.name })}</DialogTitle>
					<DialogDescription>
						{preview ? count(preview) : t("editor.rename.everywhere")}
					</DialogDescription>
				</DialogHeader>
				<DialogBody>
					{preview ? (
						<ul className="max-h-64 overflow-auto font-mono text-xs">
							{preview.map((file) => (
								<li key={file.path} className="flex justify-between gap-4 py-0.5">
									<span className="truncate">{file.path}</span>
									<span className="text-muted-foreground shrink-0">{file.edits.length}</span>
								</li>
							))}
						</ul>
					) : (
						<input
							// The dialog is the rename: its field is what to type in.
							// oxlint-disable-next-line jsx-a11y/no-autofocus
							autoFocus
							aria-label={t("editor.rename.newName")}
							value={name}
							onChange={(event) => setName(event.target.value)}
							onKeyDown={(event) => {
								if (event.key === "Enter" && name && name !== request.name) void ask();
							}}
							className="border-input bg-background focus-visible:ring-ring h-9 w-full rounded-md border px-2.5 font-mono text-sm outline-none focus-visible:ring-2"
						/>
					)}
				</DialogBody>
				<DialogFooter>
					<Button variant="ghost" onClick={onClose}>
						{t("actions.cancel")}
					</Button>
					{preview ? (
						<Button disabled={busy || !preview.length} onClick={() => void apply()}>
							{t("editor.rename.apply")}
						</Button>
					) : (
						<Button disabled={busy || !name || name === request.name} onClick={() => void ask()}>
							{t("editor.rename.preview")}
						</Button>
					)}
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

/** ⌘.: the fixes and refactorings offered at the cursor. */
export function CodeActionsDialog({
	request,
	onClose,
}: {
	request: CodeActionRequest;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const [busy, setBusy] = useState(false);
	const run = async (index: number) => {
		const action = request.actions[index];
		if (!action) return;
		setBusy(true);
		try {
			const edits = await action.resolve();
			if (edits.length) await request.apply(edits);
			onClose();
		} catch (error) {
			toastError(error instanceof Error ? error : new Error(String(error)));
			setBusy(false);
		}
	};
	const sorted = request.actions
		.map((action, index) => ({ action, index }))
		.toSorted((a, b) => Number(Boolean(b.action.preferred)) - Number(Boolean(a.action.preferred)));
	return (
		<Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
			<DialogContent size="md">
				<DialogHeader>
					<DialogTitle>{t("editor.fixes.title")}</DialogTitle>
					<DialogDescription>
						{request.actions.length ? t("editor.fixes.offered") : t("editor.fixes.nothing")}
					</DialogDescription>
				</DialogHeader>
				<DialogBody className="px-4">
					<ul className="text-sm">
						{sorted.map(({ action, index }) => (
							<li key={`${index}:${action.title}`}>
								<button
									type="button"
									disabled={busy}
									onClick={() => void run(index)}
									className="hover:bg-accent focus-visible:bg-accent flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left outline-none"
								>
									{action.preferred ? <CircleAlert className="text-warning size-3.5" /> : null}
									<span className="min-w-0 flex-1">{action.title}</span>
									{action.source ? (
										<span className="text-muted-foreground text-xs">{action.source}</span>
									) : null}
								</button>
							</li>
						))}
					</ul>
				</DialogBody>
			</DialogContent>
		</Dialog>
	);
}
