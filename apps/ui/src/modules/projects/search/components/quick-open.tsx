import { Search } from "lucide-react";
import { Dialog as Primitive } from "radix-ui";
import { useDeferredValue, useMemo, useState } from "react";

import { useFileIndex } from "@/modules/projects/search/api";
import { type Found, rank } from "@/modules/projects/search/model";
import { formatNumber, useTranslation } from "@/shared/i18n";
import { commandKey } from "@/shared/platform";
import { recentFiles } from "@/shared/preferences/store";
import { FileIcon } from "@/shared/ui/file-icon";

/**
 * Quick open (⌘P): a project's files by name, matched fuzzily as you
 * type; recently opened files first while the field is empty.
 */
export function QuickOpen({
	projectId,
	open,
	onOpenChange,
	onOpen,
}: {
	projectId: string;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	/** `beside`: open it to the side of the current window (⌘/Ctrl+Enter, ⌘/Ctrl-click). */
	onOpen: (path: string, beside: boolean) => void;
}) {
	const { t } = useTranslation("projects");
	const index = useFileIndex(projectId, open);
	const [query, setQuery] = useState("");
	const [active, setActive] = useState(0);
	const paths = index.data?.paths;
	// Ranking a large repository's files follows the typing, never holds it up.
	const wanted = useDeferredValue(query);
	const matches = useMemo<Found[]>(() => {
		if (!wanted.trim()) {
			const known = new Set(paths ?? []);
			return recentFiles(projectId)
				.filter((path) => known.has(path))
				.slice(0, 20)
				.map((path) => ({ path, score: 0, positions: [] }));
		}
		return rank(wanted, paths ?? []);
	}, [wanted, paths, projectId]);
	const close = () => {
		onOpenChange(false);
		setQuery("");
		setActive(0);
	};
	const pick = (found: Found | undefined, beside = false) => {
		if (!found) return;
		close();
		onOpen(found.path, beside);
	};

	return (
		<Primitive.Root open={open} onOpenChange={(value) => (value ? onOpenChange(true) : close())}>
			<Primitive.Portal>
				<Primitive.Overlay className="fixed inset-0 z-40 bg-black/40" />
				<Primitive.Content
					aria-describedby={undefined}
					className="bg-popover border-border fixed top-[14vh] left-1/2 z-50 w-[calc(100vw-2rem)] max-w-xl -translate-x-1/2 overflow-hidden rounded-xl border shadow-[0_16px_48px_rgba(0,0,0,0.4)] outline-none"
				>
					<Primitive.Title className="sr-only">{t("quickOpen.title")}</Primitive.Title>
					<div className="border-border flex items-center gap-2 border-b px-3">
						<Search className="text-muted-foreground size-4" />
						<input
							// oxlint-disable-next-line jsx-a11y/no-autofocus
							autoFocus
							value={query}
							onChange={(event) => {
								setQuery(event.target.value);
								setActive(0);
							}}
							onKeyDown={(event) => {
								if (event.key === "ArrowDown") {
									event.preventDefault();
									setActive((i) => Math.min(i + 1, matches.length - 1));
								} else if (event.key === "ArrowUp") {
									event.preventDefault();
									setActive((i) => Math.max(i - 1, 0));
								} else if (event.key === "Enter") {
									event.preventDefault();
									pick(matches[active], event.metaKey || event.ctrlKey);
								}
							}}
							placeholder={t("quickOpen.placeholder")}
							aria-label={t("quickOpen.title")}
							className="placeholder:text-muted-foreground h-11 flex-1 bg-transparent text-sm outline-none"
						/>
					</div>
					<ul aria-label={t("quickOpen.files")} className="max-h-[50vh] overflow-auto p-1.5">
						{matches.map((found, i) => {
							const slash = found.path.lastIndexOf("/");
							return (
								<li key={found.path}>
									<button
										type="button"
										aria-current={i === active ? "true" : undefined}
										onMouseEnter={() => setActive(i)}
										onClick={(event) => pick(found, event.metaKey || event.ctrlKey)}
										className={
											i === active
												? "bg-accent flex w-full items-center gap-2.5 rounded-md px-3 py-1.5 text-left text-sm"
												: "flex w-full items-center gap-2.5 rounded-md px-3 py-1.5 text-left text-sm"
										}
									>
										<FileIcon path={found.path} />
										<Marked
											text={found.path.slice(slash + 1)}
											positions={found.positions}
											offset={slash + 1}
										/>
										<span className="text-muted-foreground min-w-0 flex-1 truncate text-xs">
											<Marked
												text={found.path.slice(0, Math.max(0, slash))}
												positions={found.positions}
												offset={0}
											/>
										</span>
									</button>
								</li>
							);
						})}
						{!index.isPending && matches.length === 0 ? (
							<li className="text-muted-foreground px-3 py-2 text-sm">
								{query.trim() ? t("quickOpen.noMatch") : t("quickOpen.empty")}
							</li>
						) : null}
						{index.isPending ? (
							<li className="text-muted-foreground px-3 py-2 text-sm">{t("quickOpen.listing")}</li>
						) : null}
						{index.data?.truncated ? (
							<li className="text-muted-foreground px-3 py-1 text-xs">
								{t("quickOpen.truncated", { limit: formatNumber(100_000) })}
							</li>
						) : null}
					</ul>
					<p className="text-muted-foreground border-border border-t px-3 py-1.5 text-[0.6875rem]">
						{t("quickOpen.hint", { key: commandKey })}
					</p>
				</Primitive.Content>
			</Primitive.Portal>
		</Primitive.Root>
	);
}

/** Text with the matched letters highlighted. */
function Marked({
	text,
	positions,
	offset,
}: {
	text: string;
	positions: number[];
	offset: number;
}) {
	const marked = new Set(positions.map((p) => p - offset));
	return (
		<span>
			{[...text].map((letter, i) =>
				marked.has(i) ? (
					// A letter's place in the path is its identity.
					// oxlint-disable-next-line react/no-array-index-key
					<span key={i} className="text-primary font-semibold">
						{letter}
					</span>
				) : (
					letter
				),
			)}
		</span>
	);
}
