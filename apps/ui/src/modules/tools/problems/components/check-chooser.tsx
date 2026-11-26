import { FolderGit2, GitBranch } from "lucide-react";
import { useState } from "react";

import { type Activity, type Checkout, checkoutState } from "@/modules/tools/problems/model";
import type { CheckRootDto } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
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

/** A repository's checkouts that have something to check, with their roots. */
interface Choice {
	repository: string;
	checkouts: { path: string; label: string; roots: CheckRootDto[]; worktree: boolean }[];
}

/** The checkouts with roots, grouped by repository, in the order the Git view lists them. */
function choices(roots: readonly CheckRootDto[], checkouts: readonly Checkout[]): Choice[] {
	const groups = new Map<string, Choice>();
	const known = [...checkouts, { path: "", repository: "" }];
	for (const checkout of known) {
		const mine = roots.filter((root) => root.checkout === checkout.path);
		if (!mine.length) continue;
		const repository = checkout.repository ?? checkout.path;
		const group = groups.get(repository) ?? { repository, checkouts: [] };
		group.checkouts.push({
			path: checkout.path,
			label: checkout.path.startsWith(`${repository}/`)
				? checkout.path.slice(repository.length + 1)
				: checkout.path,
			roots: mine,
			worktree: checkout.path !== repository,
		});
		groups.set(repository, group);
	}
	return [...groups.values()];
}

/**
 * Check project's chooser: with more roots than a few, which
 * repositories and worktrees to check, the active and pinned ones chosen
 * to begin with. The machine runs a few checks at once; the rest wait.
 */
export function CheckChooser({
	open,
	onOpenChange,
	roots,
	checkouts,
	activity,
	onCheck,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	roots: readonly CheckRootDto[];
	checkouts: readonly Checkout[];
	activity: readonly Activity[];
	onCheck: (checkouts: string[]) => void;
}) {
	const { t } = useTranslation("tools");
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent size="lg">
				<DialogHeader>
					<DialogTitle>{t("checkChooser.title")}</DialogTitle>
					<DialogDescription>
						{t("checkChooser.description", { count: roots.length })}
					</DialogDescription>
				</DialogHeader>
				{/* Remounted each time it opens, so it starts from the active ones. */}
				{open ? (
					<ChooserForm roots={roots} checkouts={checkouts} activity={activity} onCheck={onCheck} />
				) : null}
			</DialogContent>
		</Dialog>
	);
}

function ChooserForm({
	roots,
	checkouts,
	activity,
	onCheck,
}: {
	roots: readonly CheckRootDto[];
	checkouts: readonly Checkout[];
	activity: readonly Activity[];
	onCheck: (checkouts: string[]) => void;
}) {
	const { t } = useTranslation("tools");
	const groups = choices(roots, checkouts);
	const all = groups.flatMap((group) => group.checkouts.map((checkout) => checkout.path));
	const [chosen, setChosen] = useState<ReadonlySet<string>>(
		() =>
			new Set(
				checkouts
					.filter((checkout) => checkoutState(checkout, activity).active)
					.map((checkout) => checkout.path)
					.filter((path) => all.includes(path)),
			),
	);
	const count = [...chosen].reduce(
		(sum, path) => sum + roots.filter((root) => root.checkout === path).length,
		0,
	);
	const flip = (path: string) =>
		setChosen((now) => {
			const next = new Set(now);
			if (next.has(path)) next.delete(path);
			else next.add(path);
			return next;
		});
	return (
		<form
			className="flex min-h-0 flex-col"
			onSubmit={(event) => {
				event.preventDefault();
				onCheck([...chosen]);
			}}
		>
			<DialogBody className="gap-3">
				<div className="flex gap-2 text-xs">
					<button
						type="button"
						className="text-primary hover:underline"
						onClick={() => setChosen(new Set(all))}
					>
						{t("checkChooser.all")}
					</button>
					<button
						type="button"
						className="text-primary hover:underline"
						onClick={() => setChosen(new Set())}
					>
						{t("checkChooser.none")}
					</button>
				</div>
				<ul
					aria-label={t("checkChooser.list")}
					className="border-border overflow-auto rounded-md border p-1 text-[0.8125rem]"
				>
					{groups.map((group) => (
						<li key={group.repository}>
							{group.checkouts.length > 1 || group.checkouts[0]?.worktree ? (
								<p className="text-muted-foreground flex items-center gap-1 px-1 pt-1 text-xs font-medium">
									<FolderGit2 className="size-3.5" />
									{group.repository || t("checkChooser.project")}
								</p>
							) : null}
							<ul>
								{group.checkouts.map((checkout) => (
									<li key={checkout.path}>
										<label className="hover:bg-accent/60 flex items-center gap-2 rounded px-1 py-0.5">
											<input
												type="checkbox"
												checked={chosen.has(checkout.path)}
												onChange={() => flip(checkout.path)}
											/>
											{checkout.worktree ? (
												<GitBranch className="text-muted-foreground size-3.5" />
											) : (
												<FolderGit2 className="text-muted-foreground size-3.5" />
											)}
											<span className="min-w-0 flex-1 truncate">
												{checkout.worktree
													? checkout.label || t("checkChooser.outsideRepositories")
													: checkout.path || t("checkChooser.outsideRepositories")}
											</span>
											<span className="text-muted-foreground shrink-0 text-[0.6875rem]">
												{checkout.roots.length === 1
													? checkout.roots[0]!.name
													: t("checkChooser.roots", { count: checkout.roots.length })}
											</span>
										</label>
									</li>
								))}
							</ul>
						</li>
					))}
				</ul>
			</DialogBody>
			<DialogFooter>
				<Button type="submit" disabled={!chosen.size}>
					{chosen.size
						? t("checkChooser.submit", {
								count: chosen.size,
								roots: t("checkChooser.roots", { count }),
							})
						: t("checkChooser.chooseFirst")}
				</Button>
			</DialogFooter>
		</form>
	);
}
