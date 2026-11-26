import { cn } from "cn";
import { Pin, PinOff } from "lucide-react";

import { usePinRepositoryOf, useRepositoryActivity } from "@/modules/tools/problems/api";
import { useActivityEvents } from "@/modules/tools/problems/stream";
import { explain } from "@/shared/api/problems";
import { useTranslation } from "@/shared/i18n";

/**
 * Pin or Unpin a repository (or one worktree) of a project: a
 * pinned one stays active, shows in Problems and, when the project says
 * so, is where language servers start.
 */
export function PinButton({
	projectId,
	repository,
	worktree,
	pinned,
	className,
}: {
	projectId: string;
	repository: string;
	/** A worktree of it; the whole repository when absent. */
	worktree?: string | null;
	pinned: boolean;
	className?: string;
}) {
	const { t } = useTranslation("tools");
	const pin = usePinRepositoryOf(projectId);
	const name = worktree ?? (repository || t("pin.theProject"));
	return (
		<button
			type="button"
			aria-label={pinned ? t("pin.unpin", { name }) : t("pin.pin", { name })}
			aria-pressed={pinned}
			title={
				pin.isError
					? explain(pin.error, t("pin.failed"))
					: pinned
						? t("pin.unpinHelp")
						: t("pin.pinHelp")
			}
			disabled={pin.isPending}
			onClick={(event) => {
				event.stopPropagation();
				pin.mutate({
					id: projectId,
					data: { repository, worktree: worktree ?? null, pinned: !pinned },
				});
			}}
			className={cn(
				"hover:bg-accent focus-visible:ring-ring grid size-5 shrink-0 place-items-center rounded outline-none focus-visible:ring-2 disabled:opacity-40 [&_svg]:size-3",
				pinned ? "text-primary" : "text-muted-foreground",
				className,
			)}
		>
			{pinned ? <PinOff /> : <Pin />}
		</button>
	);
}

/**
 * Pin or Unpin for a repository or worktree wherever repositories are
 * listed (the Git view): whether it is pinned comes from the project's
 * activity.
 */
export function RepositoryPin({
	projectId,
	repository,
	worktree,
}: {
	projectId: string;
	repository: string;
	worktree?: string | null;
}) {
	const activity = useRepositoryActivity(projectId);
	useActivityEvents(projectId);
	const pinned = (activity.data?.items ?? []).some(
		(row) =>
			row.repository === repository && (row.worktree ?? null) === (worktree ?? null) && row.pinned,
	);
	return (
		<PinButton
			projectId={projectId}
			repository={repository}
			worktree={worktree}
			pinned={pinned}
			className="size-6"
		/>
	);
}
