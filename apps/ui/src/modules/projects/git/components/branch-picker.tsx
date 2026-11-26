import { cn } from "cn";
import { ArrowDown, ArrowUp, Check, GitBranch, Pencil, Plus, Trash2 } from "lucide-react";
import { useState } from "react";

import { GLANCE, useBranches, useGitActions, useGitStatus } from "@/modules/projects/git/api";
import { problemCode } from "@/shared/api/fetch";
import type { GitBranch as Branch, Project } from "@/shared/api/generated/ahab.schemas";
import { Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";
import { NameDialog } from "@/shared/ui/name-dialog";
import { toast, toastError } from "@/shared/ui/toast";

/**
 * The current branch in the status bar (`⎇ main ↑1 ↓2`); it opens the
 * branch picker. Nothing shows outside a Git repository.
 */
export function BranchButton({
	project,
	repository,
	onOpen,
	className,
}: {
	project: Project;
	repository?: string;
	onOpen: () => void;
	/** Its look where it sits (the status bar's). */
	className?: string;
}) {
	const { t } = useTranslation("projects");
	const status = useGitStatus(project.id, repository, project.availability === "available", GLANCE);
	const data = status.data;
	if (!data?.repository) return null;
	return (
		<button
			type="button"
			onClick={onOpen}
			aria-label={t("branchButton.label", { branch: data.branch ?? t("branchButton.detached") })}
			title={t("branchButton.title")}
			className={cn(
				"text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring flex h-7 min-w-0 items-center gap-1 rounded-md px-2 text-sm outline-none focus-visible:ring-2",
				className,
			)}
		>
			<GitBranch className="size-3.5 shrink-0" />
			<span className="truncate">{data.branch ?? data.head ?? t("branchButton.detached")}</span>
			{data.ahead ? (
				<span className="flex shrink-0 items-center text-xs">
					<ArrowUp className="size-3" />
					{data.ahead}
				</span>
			) : null}
			{data.behind ? (
				<span className="flex shrink-0 items-center text-xs">
					<ArrowDown className="size-3" />
					{data.behind}
				</span>
			) : null}
		</button>
	);
}

/** Switch, create, rename or delete branches. */
export function BranchPicker({
	project,
	repository,
	open,
	onOpenChange,
}: {
	project: Project;
	repository?: string;
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	const { t } = useTranslation("projects");
	const id = project.id;
	const status = useGitStatus(id, repository, open);
	const branches = useBranches(id, repository, open);
	const repo = repository ? { repository } : undefined;
	const actions = useGitActions(id);
	const [query, setQuery] = useState("");
	const [pending, setPending] = useState<{ branch: string; remote: boolean } | null>(null);
	const [renaming, setRenaming] = useState<string | null>(null);
	const [deleting, setDeleting] = useState<{ name: string; force: boolean } | null>(null);
	const readOnly = project.mode === "readOnly";
	const data = status.data;
	const dirty = Boolean(
		data && data.staged.length + data.changes.length + data.untracked.length > 0,
	);
	const q = query.trim().toLowerCase();
	const local = (branches.data?.local ?? []).filter((b) => b.name.toLowerCase().includes(q));
	const remote = (branches.data?.remote ?? []).filter((b) => b.name.toLowerCase().includes(q));
	const exists = (branches.data?.local ?? []).some((b) => b.name === query.trim());
	const close = () => {
		onOpenChange(false);
		setQuery("");
		setPending(null);
	};

	const switchTo = (branch: string, remoteBranch: boolean, stash: boolean) =>
		actions.switchBranch.mutate(
			{ id, params: repo, data: { branch, remote: remoteBranch, stash } },
			{
				onSuccess: () => {
					toast(
						stash
							? t("branchPicker.switchedStashed", { branch })
							: t("branchPicker.switched", { branch }),
					);
					close();
				},
				onError: (error) => {
					if (problemCode(error) === "would_overwrite_changes")
						setPending({ branch, remote: remoteBranch });
					toastError(error);
				},
			},
		);
	const choose = (branch: Branch, remoteBranch: boolean) => {
		if (!remoteBranch && branch.name === branches.data?.current) return close();
		if (dirty) setPending({ branch: branch.name, remote: remoteBranch });
		else switchTo(branch.name, remoteBranch, false);
	};
	const create = () =>
		actions.createBranch.mutate(
			{ id, params: repo, data: { name: query.trim() } },
			{
				onSuccess: () => {
					toast(t("branchPicker.created", { branch: query.trim() }));
					close();
				},
				onError: toastError,
			},
		);

	return (
		<>
			<Dialog open={open} onOpenChange={(next) => (next ? onOpenChange(true) : close())}>
				<DialogContent size="lg">
					<DialogTitle className="sr-only">{t("branchPicker.title")}</DialogTitle>
					{pending ? (
						<>
							<DialogHeader className="pb-6">
								<p className="font-heading text-heading text-lg leading-tight font-bold">
									{t("branchPicker.dirty.title", { branch: pending.branch })}
								</p>
								<DialogDescription>
									{t("branchPicker.dirty.description", { branch: pending.branch })}
								</DialogDescription>
							</DialogHeader>
							<DialogFooter>
								<Button variant="ghost" onClick={() => setPending(null)}>
									{t("actions.cancel", { ns: "common" })}
								</Button>
								<Button
									variant="outline"
									onClick={() => switchTo(pending.branch, pending.remote, true)}
								>
									{t("branchPicker.dirty.stash")}
								</Button>
								<Button onClick={() => switchTo(pending.branch, pending.remote, false)}>
									{t("branchPicker.dirty.carry")}
								</Button>
							</DialogFooter>
						</>
					) : (
						<>
							<input
								aria-label={t("branchPicker.query.label")}
								autoFocus
								value={query}
								onChange={(event) => setQuery(event.target.value)}
								onKeyDown={(event) => {
									if (event.key === "Enter" && query.trim() && !exists && !readOnly) create();
								}}
								placeholder={t("branchPicker.query.placeholder")}
								className="border-border placeholder:text-muted-foreground w-full shrink-0 border-b bg-transparent py-3.5 pr-14 pl-4 text-sm outline-none"
							/>
							<div className="min-h-0 overflow-auto p-1.5">
								{query.trim() && !exists && !readOnly ? (
									<Item onClick={create} icon={<Plus />}>
										<Trans
											t={t}
											i18nKey="branchPicker.create"
											values={{ name: query.trim(), base: branches.data?.current ?? "HEAD" }}
											components={{ name: <span className="text-heading font-medium" /> }}
										/>
									</Item>
								) : null}
								<Label>{t("branchPicker.branches")}</Label>
								{local.map((branch) => (
									<Item
										key={branch.name}
										onClick={() => choose(branch, false)}
										icon={branch.name === branches.data?.current ? <Check /> : <GitBranch />}
										note={branch.subject}
										actions={
											readOnly ? null : (
												<>
													<Mini
														label={t("branchPicker.rename", { branch: branch.name })}
														onClick={() => setRenaming(branch.name)}
													>
														<Pencil />
													</Mini>
													{branch.name === branches.data?.current ? null : (
														<Mini
															label={t("branchPicker.delete", { branch: branch.name })}
															onClick={() => setDeleting({ name: branch.name, force: false })}
														>
															<Trash2 />
														</Mini>
													)}
												</>
											)
										}
									>
										{branch.name}
									</Item>
								))}
								{remote.length ? <Label>{t("branchPicker.remoteBranches")}</Label> : null}
								{remote.map((branch) => (
									<Item
										key={branch.name}
										onClick={() => choose(branch, true)}
										icon={<GitBranch />}
										note={branch.subject}
									>
										{branch.name}
									</Item>
								))}
								{branches.isPending ? (
									<p className="text-muted-foreground px-3 py-2 text-sm">
										{t("state.loading", { ns: "common" })}
									</p>
								) : null}
							</div>
						</>
					)}
				</DialogContent>
			</Dialog>
			<NameDialog
				open={renaming !== null}
				onOpenChange={(next) => !next && setRenaming(null)}
				title={t("branchPicker.renameDialog.title", { branch: renaming ?? "" })}
				label={t("branchPicker.renameDialog.label")}
				initial={renaming ?? ""}
				action={t("branchPicker.renameDialog.action")}
				pending={actions.renameBranch.isPending}
				error={actions.renameBranch.error?.message}
				onSubmit={(name) =>
					actions.renameBranch.mutate(
						{ id, params: repo, data: { from: renaming ?? "", to: name } },
						{ onSuccess: () => setRenaming(null) },
					)
				}
			/>
			<ConfirmDialog
				open={deleting !== null}
				onOpenChange={(next) => !next && setDeleting(null)}
				title={
					deleting?.force
						? t("branchPicker.deleteDialog.titleNotMerged", { branch: deleting.name })
						: t("branchPicker.deleteDialog.title", { branch: deleting?.name ?? "" })
				}
				description={
					deleting?.force
						? t("branchPicker.deleteDialog.descriptionNotMerged")
						: t("branchPicker.deleteDialog.description")
				}
				action={t("actions.delete", { ns: "common" })}
				destructive
				pending={actions.deleteBranch.isPending}
				onConfirm={() => {
					if (!deleting) return;
					actions.deleteBranch.mutate(
						{ id, params: repo, data: { name: deleting.name, force: deleting.force } },
						{
							onSuccess: () => setDeleting(null),
							onError: (error) => {
								if (problemCode(error) === "branch_not_merged")
									setDeleting({ ...deleting, force: true });
								else {
									toastError(error);
									setDeleting(null);
								}
							},
						},
					);
				}}
			/>
		</>
	);
}

function Label({ children }: { children: React.ReactNode }) {
	return (
		<p className="text-muted-foreground px-3 pt-2 pb-1 text-[0.6875rem] font-semibold tracking-wider uppercase">
			{children}
		</p>
	);
}

function Item({
	onClick,
	icon,
	note,
	actions,
	children,
}: {
	onClick: () => void;
	icon: React.ReactNode;
	note?: string;
	actions?: React.ReactNode;
	children: React.ReactNode;
}) {
	return (
		<div className="group hover:bg-accent flex items-center rounded-md">
			<button
				type="button"
				onClick={onClick}
				className="[&_svg]:text-muted-foreground flex min-w-0 flex-1 items-center gap-2.5 px-3 py-2 text-left text-sm outline-none [&_svg]:size-4 [&_svg]:shrink-0"
			>
				{icon}
				<span className="min-w-0 truncate">{children}</span>
				{note ? (
					<span className="text-muted-foreground min-w-0 flex-1 truncate text-xs">{note}</span>
				) : null}
			</button>
			{actions ? (
				<span className={cn("flex gap-0.5 pr-1.5 md:opacity-0 md:group-hover:opacity-100")}>
					{actions}
				</span>
			) : null}
		</div>
	);
}

function Mini({
	label,
	onClick,
	children,
}: {
	label: string;
	onClick: () => void;
	children: React.ReactNode;
}) {
	return (
		<button
			type="button"
			aria-label={label}
			title={label}
			onClick={onClick}
			className="text-muted-foreground hover:text-foreground hover:bg-background grid size-6 place-items-center rounded [&_svg]:size-3.5"
		>
			{children}
		</button>
	);
}
