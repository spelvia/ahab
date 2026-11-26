import { Archive, ArrowUp, Check, Minus } from "lucide-react";
import { useState } from "react";

import { useGitActions, useStashes } from "@/modules/projects/git/api";
import { Chevron } from "@/modules/projects/git/components/changes";
import { ago } from "@/shared/format";
import { useTranslation } from "@/shared/i18n";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import { IconButton } from "@/shared/ui/icon-button";
import { toastError } from "@/shared/ui/toast";

/** The stashes: apply, pop or drop each. */
export function Stashes({
	projectId,
	repository,
	readOnly,
}: {
	projectId: string;
	repository?: string;
	readOnly: boolean;
}) {
	const { t } = useTranslation("projects");
	const [open, setOpen] = useState(false);
	const stashes = useStashes(projectId, repository, open);
	const repo = repository ? { repository } : undefined;
	const actions = useGitActions(projectId);
	const [dropping, setDropping] = useState<number | null>(null);
	return (
		<section aria-label={t("stashes.title")}>
			<div className="flex h-7 items-center pl-1">
				<Chevron
					open={open}
					onClick={() => setOpen((value) => !value)}
					label={t("stashes.title")}
				/>
				<button
					type="button"
					onClick={() => setOpen((value) => !value)}
					className="text-muted-foreground hover:text-foreground flex-1 pl-1 text-left text-[0.6875rem] font-semibold tracking-wider uppercase"
				>
					{t("stashes.title")}
				</button>
			</div>
			{open ? (
				<ul>
					{(stashes.data?.items ?? []).map((stash) => (
						<li
							key={stash.index}
							className="group/row hover:bg-accent/60 flex h-7 items-center gap-2 pr-1.5 pl-6 text-[0.8125rem]"
						>
							<Archive className="text-muted-foreground size-3.5 shrink-0" />
							<span className="min-w-0 flex-1 truncate" title={stash.message}>
								{stash.message}
							</span>
							{readOnly ? null : (
								<span className="flex gap-0.5 md:hidden md:group-focus-within/row:flex md:group-hover/row:flex">
									<IconButton
										label={t("stashes.apply")}
										onClick={() =>
											actions.stashApply.mutate(
												{ id: projectId, index: stash.index, params: repo },
												{ onError: toastError },
											)
										}
									>
										<Check />
									</IconButton>
									<IconButton
										label={t("stashes.pop")}
										onClick={() =>
											actions.stashApply.mutate(
												{ id: projectId, index: stash.index, params: { ...repo, pop: true } },
												{ onError: toastError },
											)
										}
									>
										<ArrowUp />
									</IconButton>
									<IconButton label={t("stashes.drop")} onClick={() => setDropping(stash.index)}>
										<Minus />
									</IconButton>
								</span>
							)}
							<span className="text-muted-foreground text-[0.6875rem]">
								{ago(stash.time * 1000)}
							</span>
						</li>
					))}
					{stashes.data?.items.length === 0 ? (
						<p className="text-muted-foreground px-6 py-1 text-xs">{t("stashes.empty")}</p>
					) : null}
				</ul>
			) : null}
			<ConfirmDialog
				open={dropping !== null}
				onOpenChange={(next) => !next && setDropping(null)}
				title={t("stashes.dropDialog.title")}
				description={t("stashes.dropDialog.description")}
				action={t("stashes.dropDialog.action")}
				destructive
				onConfirm={() => {
					if (dropping !== null)
						actions.stashDrop.mutate(
							{ id: projectId, index: dropping, params: repo },
							{ onError: toastError },
						);
					setDropping(null);
				}}
			/>
		</section>
	);
}
