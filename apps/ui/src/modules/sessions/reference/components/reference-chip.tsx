import { cn } from "cn";
import { FileDiff, MessagesSquare, Search, SquareTerminal, X } from "lucide-react";

import { useTranslation } from "@/shared/i18n";
import { FileIcon } from "@/shared/ui/file-icon";

/** One piece of context, as a chip: in a composer (removable) or a sent message. */
export function ReferenceChip({
	kind,
	label,
	path,
	title,
	stale,
	onOpen,
	onRemove,
}: {
	kind: string;
	label: string;
	path?: string;
	/** More, on hover: the first lines of the text. */
	title?: string;
	stale?: boolean;
	onOpen?: () => void;
	onRemove?: () => void;
}) {
	const { t } = useTranslation("sessions");
	const icon =
		kind === "diffHunk" ? (
			<FileDiff className="size-3.5 shrink-0" />
		) : kind === "terminalOutput" ? (
			<SquareTerminal className="size-3.5 shrink-0" />
		) : kind === "searchResults" ? (
			<Search className="size-3.5 shrink-0" />
		) : kind === "chatHistory" ? (
			<MessagesSquare className="size-3.5 shrink-0" />
		) : (
			<FileIcon path={path ?? label} className="size-3.5 shrink-0" />
		);
	const body = (
		<>
			{icon}
			<span className="truncate font-mono text-[0.7188rem]">{label}</span>
			{stale ? (
				<span className="text-warning shrink-0 text-[0.6562rem]" title={t("references.staleTitle")}>
					{t("references.stale")}
				</span>
			) : null}
		</>
	);
	return (
		<span
			title={title}
			className="border-border bg-card text-foreground inline-flex h-6 max-w-72 items-center gap-1 rounded-md border pr-1 pl-1.5 text-xs"
		>
			{onOpen ? (
				<button
					type="button"
					onClick={onOpen}
					className="focus-visible:ring-ring flex min-w-0 items-center gap-1 rounded-sm outline-none hover:underline focus-visible:ring-2"
				>
					{body}
				</button>
			) : (
				<span className="flex min-w-0 items-center gap-1">{body}</span>
			)}
			{onRemove ? (
				<button
					type="button"
					onClick={onRemove}
					aria-label={t("references.remove", { label })}
					className={cn(
						"text-muted-foreground hover:text-foreground focus-visible:ring-ring grid size-4 shrink-0 place-items-center rounded-sm outline-none focus-visible:ring-2",
					)}
				>
					<X className="size-3" />
				</button>
			) : null}
		</span>
	);
}

/** The first lines of a reference's text, for a tooltip. */
export function preview(text: string | undefined): string | undefined {
	if (!text) return undefined;
	const lines = text.split("\n");
	return lines.length > 12 ? `${lines.slice(0, 12).join("\n")}\n…` : text;
}
