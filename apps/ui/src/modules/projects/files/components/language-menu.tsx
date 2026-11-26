import { cn } from "cn";
import { Check, ChevronDown } from "lucide-react";

import { useTranslation } from "@/shared/i18n";
import type { Language } from "@/shared/syntax/model";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

/**
 * A file window's language: what it shows as, and the menu to
 * choose another, for this window or for every file of its kind (`*.h`) in
 * this browser.
 */
export function LanguageMenu({
	languages,
	detected,
	chosen,
	pattern,
	patternChosen,
	note,
	onChoose,
	onChoosePattern,
	triggerClassName,
	align,
}: {
	/** Every language that may be chosen. */
	languages: Language[];
	/** What the file is found to be; none: plain text. */
	detected: Language | undefined;
	/** A language chosen for this window: an ID, or `plaintext`. */
	chosen: string | undefined;
	/** The pattern of files like this one: `*.h`, `Makefile`. */
	pattern: string;
	/** Whether a language was chosen for that pattern. */
	patternChosen: boolean;
	/** Why it shows plain (too large), if it does. */
	note?: string;
	onChoose: (language: string | undefined) => void;
	onChoosePattern: (language: string | undefined) => void;
	/** The trigger's look, in place of the window's (the status bar's); no chevron. */
	triggerClassName?: string;
	align?: "start" | "end";
}) {
	const { t } = useTranslation("projects");
	const shown = chosen ? languages.find((language) => language.id === chosen) : detected;
	const name = shown?.name ?? t("languageMenu.plainText");
	const choosable = languages
		.filter((language) => !language.declaration.hidden && language.wasm)
		.toSorted((a, b) => a.name.localeCompare(b.name));
	const effective = chosen ?? detected?.id ?? "plaintext";
	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				title={note ?? t("languageMenu.triggerTitle")}
				aria-label={t("languageMenu.triggerLabel", { name })}
				className={cn(
					triggerClassName ??
						"text-muted-foreground hover:text-foreground hover:bg-accent/60 data-[state=open]:bg-accent focus-visible:ring-ring flex h-6 items-center gap-1 rounded-md px-2 outline-none focus-visible:ring-2",
				)}
			>
				{name}
				{note ? <span className="text-warning">{t("languageMenu.plainMark")}</span> : null}
				{triggerClassName ? null : <ChevronDown className="size-3 opacity-60" />}
			</DropdownMenuTrigger>
			<DropdownMenuContent align={align} className="max-h-96 w-60 overflow-auto text-xs">
				{note ? <p className="text-muted-foreground px-2 py-1.5 text-xs">{note}</p> : null}
				<DropdownMenuItem onSelect={() => onChoose(undefined)} className="text-xs">
					<span className="flex-1">
						{t("languageMenu.detect", { name: detected?.name ?? t("languageMenu.plainText") })}
					</span>
					{chosen ? null : <Check />}
				</DropdownMenuItem>
				<DropdownMenuSeparator />
				<DropdownMenuItem onSelect={() => onChoose("plaintext")} className="text-xs">
					<span className="flex-1">{t("languageMenu.plainText")}</span>
					{chosen === "plaintext" ? <Check /> : null}
				</DropdownMenuItem>
				{choosable.map((language) => (
					<DropdownMenuItem
						key={language.id}
						onSelect={() => onChoose(language.id)}
						className="text-xs"
					>
						<span className="flex-1">{language.name}</span>
						{language.source !== "bundled" ? (
							<span className="text-muted-foreground text-[0.6875rem]">{language.source}</span>
						) : null}
						{chosen === language.id ? <Check /> : null}
					</DropdownMenuItem>
				))}
				<DropdownMenuSeparator />
				{patternChosen ? (
					<DropdownMenuItem onSelect={() => onChoosePattern(undefined)} className="text-xs">
						{t("languageMenu.forgetPattern", { pattern })}
					</DropdownMenuItem>
				) : (
					<DropdownMenuItem onSelect={() => onChoosePattern(effective)} className="text-xs">
						{t("languageMenu.choosePattern", { pattern, name })}
					</DropdownMenuItem>
				)}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
