import { cn } from "cn";
import { CornerDownLeft, Search } from "lucide-react";
import { Dialog as Primitive } from "radix-ui";
import { useMemo, useState } from "react";

import { useTranslation } from "@/shared/i18n";

export interface Command {
	id: string;
	label: string;
	/** Shown dimmed after the label, such as "Project" or a path. */
	hint?: string;
	group: string;
	run: () => void;
}

/**
 * The search bar's palette (⌘K): projects and actions, filtered
 * as you type; arrows and Enter to run one.
 */
export function CommandPalette({
	open,
	onOpenChange,
	commands,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	commands: Command[];
}) {
	const { t } = useTranslation("projects");
	const [query, setQuery] = useState("");
	const [active, setActive] = useState(0);
	const matches = useMemo(() => {
		const words = query.toLowerCase().split(/\s+/).filter(Boolean);
		return commands.filter((command) => {
			const text = `${command.label} ${command.hint ?? ""} ${command.group}`.toLowerCase();
			return words.every((word) => text.includes(word));
		});
	}, [commands, query]);
	const close = () => {
		onOpenChange(false);
		setQuery("");
		setActive(0);
	};
	const run = (command: Command | undefined) => {
		if (!command) return;
		close();
		command.run();
	};

	return (
		<Primitive.Root open={open} onOpenChange={(value) => (value ? onOpenChange(true) : close())}>
			<Primitive.Portal>
				<Primitive.Overlay className="fixed inset-0 z-40 bg-black/40" />
				<Primitive.Content
					aria-describedby={undefined}
					className="bg-popover border-border fixed top-[14vh] left-1/2 z-50 w-[calc(100vw-2rem)] max-w-xl -translate-x-1/2 overflow-hidden rounded-xl border shadow-[0_16px_48px_rgba(0,0,0,0.4)] outline-none"
				>
					<Primitive.Title className="sr-only">{t("commandPalette.title")}</Primitive.Title>
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
									setActive((index) => Math.min(index + 1, matches.length - 1));
								} else if (event.key === "ArrowUp") {
									event.preventDefault();
									setActive((index) => Math.max(index - 1, 0));
								} else if (event.key === "Enter") {
									event.preventDefault();
									run(matches[active]);
								}
							}}
							placeholder={t("commandPalette.placeholder")}
							aria-label={t("commandPalette.search")}
							className="placeholder:text-muted-foreground h-11 flex-1 bg-transparent text-sm outline-none"
						/>
					</div>
					<ul aria-label={t("commandPalette.list")} className="max-h-80 overflow-auto p-1.5">
						{matches.length === 0 ? (
							<li className="text-muted-foreground px-3 py-6 text-center text-sm">
								{t("commandPalette.empty")}
							</li>
						) : (
							matches.map((command, index) => (
								<li key={command.id}>
									<button
										type="button"
										onMouseMove={() => setActive(index)}
										onClick={() => run(command)}
										className={cn(
											"flex w-full items-center gap-3 rounded-md px-2.5 py-2 text-left text-sm outline-none",
											index === active && "bg-accent text-accent-foreground",
										)}
									>
										<span className="text-muted-foreground w-20 shrink-0 text-xs">
											{command.group}
										</span>
										<span className="truncate">{command.label}</span>
										{command.hint ? (
											<span className="text-muted-foreground truncate text-xs">{command.hint}</span>
										) : null}
										{index === active ? (
											<CornerDownLeft className="text-muted-foreground ml-auto size-3.5 shrink-0" />
										) : null}
									</button>
								</li>
							))
						)}
					</ul>
				</Primitive.Content>
			</Primitive.Portal>
		</Primitive.Root>
	);
}
