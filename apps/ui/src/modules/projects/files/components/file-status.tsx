import { Link } from "@tanstack/react-router";
import { cn } from "cn";
import { Check, Eye, Pencil } from "lucide-react";

import { LanguageMenu } from "@/modules/projects/files/components/language-menu";
import { type FileStatus, useFileStatus } from "@/modules/projects/files/hooks";
import { LanguageStatusItem } from "@/shared/editor/language-ui";
import { cursorLabel, indentationLabel, type LineEnding } from "@/shared/editor/model";
import { useTranslation } from "@/shared/i18n";
import { mac } from "@/shared/platform";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { StatusItem, statusItemClass, StatusText } from "@/shared/ui/status-bar";

/**
 * The focused file window's items in the status bar, as VS Code's: where the
 * cursor is (go to line), indentation (change it for the window), encoding,
 * line endings (convert them while editing), the language (choose another),
 * its language tools, and Read-only/Editing. Nothing when no file window is
 * focused.
 */
export function FileStatusItems({ statusKey }: { statusKey: string | undefined }) {
	const { t } = useTranslation("projects");
	const { status, cursor } = useFileStatus(statusKey);
	if (!status) return null;
	const { indentation, lineEnding } = status;
	const edit = {
		label: t(`fileStatus.edit.${status.edit}.label`),
		title: t(`fileStatus.edit.${status.edit}.title`),
	};
	const indented = indentationLabel(indentation.useTabs, indentation.tabSize);
	return (
		<>
			{cursor ? (
				<StatusItem
					label={t("fileStatus.cursor", { cursor: cursorLabel(cursor) })}
					onClick={status.goToLine}
					disabled={!status.goToLine}
				>
					{cursorLabel(cursor)}
				</StatusItem>
			) : null}
			<DropdownMenu>
				<DropdownMenuTrigger
					className={statusItemClass}
					title={t("fileStatus.indentation.title", {
						indentation: indented,
						source: t(`fileStatus.sourceInline.${indentation.source}`),
					})}
					aria-label={t("fileStatus.indentation.label", { indentation: indented })}
				>
					{indented}
				</DropdownMenuTrigger>
				<IndentationMenu status={status} />
			</DropdownMenu>
			<StatusText title={t("fileStatus.encoding")}>{status.encoding}</StatusText>
			<DropdownMenu>
				<DropdownMenuTrigger
					className={cn(statusItemClass, lineEnding.mixed && "text-warning")}
					title={
						lineEnding.mixed
							? t("fileStatus.lineEnding.mixedTitle", { ending: lineEnding.ending })
							: t("fileStatus.lineEnding.title", { ending: lineEnding.ending })
					}
					aria-label={t("fileStatus.lineEnding.label", { ending: lineEnding.ending })}
				>
					{lineEnding.ending}
				</DropdownMenuTrigger>
				<LineEndingMenu status={status} />
			</DropdownMenu>
			{status.language ? (
				<LanguageMenu
					languages={status.language.languages}
					detected={status.language.detected}
					chosen={status.language.chosen}
					pattern={status.language.pattern}
					patternChosen={status.language.patternChosen}
					note={status.language.note}
					onChoose={status.language.choose}
					onChoosePattern={status.language.choosePattern}
					triggerClassName={statusItemClass}
					align="end"
				/>
			) : status.languageName ? (
				<StatusText>{status.languageName}</StatusText>
			) : null}
			{status.languageStatus ? (
				<LanguageStatusItem status={status.languageStatus} triggerClassName={statusItemClass} />
			) : null}
			{status.toggleEdit ? (
				<StatusItem
					label={t("fileStatus.toggleEdit", { title: edit.title, shortcut: mac ? "⌘E" : "Ctrl+E" })}
					onClick={status.toggleEdit}
					aria-pressed={status.edit === "editing"}
					className={status.edit === "editing" ? "text-foreground" : undefined}
				>
					{status.edit === "editing" ? (
						<Pencil className="size-3.5" />
					) : (
						<Eye className="size-3.5" />
					)}
					{edit.label}
				</StatusItem>
			) : (
				<StatusText title={edit.title}>{edit.label}</StatusText>
			)}
		</>
	);
}

/** Change the window's indentation: tabs or spaces, how wide, or find it again. */
function IndentationMenu({ status }: { status: FileStatus }) {
	const { t } = useTranslation("projects");
	const { indentation, setIndentation } = status;
	const set = setIndentation;
	return (
		<DropdownMenuContent align="end" className="w-60 text-xs">
			<p className="text-muted-foreground px-2 py-1.5">
				{t(`fileStatus.source.${indentation.source}`)}
			</p>
			{set ? (
				<>
					<DropdownMenuItem
						className="text-xs"
						onSelect={() => set({ useTabs: false, tabSize: indentation.tabSize })}
					>
						<span className="flex-1">{t("fileStatus.indentation.useSpaces")}</span>
						{indentation.useTabs ? null : <Check />}
					</DropdownMenuItem>
					<DropdownMenuItem
						className="text-xs"
						onSelect={() => set({ useTabs: true, tabSize: indentation.tabSize })}
					>
						<span className="flex-1">{t("fileStatus.indentation.useTabs")}</span>
						{indentation.useTabs ? <Check /> : null}
					</DropdownMenuItem>
					<DropdownMenuSeparator />
					{[2, 4, 8].map((size) => (
						<DropdownMenuItem
							key={size}
							className="text-xs"
							onSelect={() => set({ useTabs: indentation.useTabs, tabSize: size })}
						>
							<span className="flex-1">
								{indentation.useTabs
									? t("fileStatus.indentation.tabSize", { size })
									: t("fileStatus.indentation.spaces", { size })}
							</span>
							{indentation.tabSize === size ? <Check /> : null}
						</DropdownMenuItem>
					))}
					<DropdownMenuSeparator />
					<DropdownMenuItem
						className="text-xs"
						disabled={indentation.source !== "window"}
						onSelect={() => set(null)}
					>
						{t("fileStatus.indentation.findAgain")}
					</DropdownMenuItem>
				</>
			) : null}
			<DropdownMenuItem asChild className="text-xs">
				<Link to="/settings/editor">{t("fileStatus.indentation.editorSettings")}</Link>
			</DropdownMenuItem>
		</DropdownMenuContent>
	);
}

/** LF or CRLF: converting changes the text, so only while editing. */
function LineEndingMenu({ status }: { status: FileStatus }) {
	const { t } = useTranslation("projects");
	const { lineEnding, setLineEnding } = status;
	const endings: { ending: LineEnding; label: string }[] = [
		{ ending: "LF", label: t("fileStatus.lineEnding.lf") },
		{ ending: "CRLF", label: t("fileStatus.lineEnding.crlf") },
	];
	return (
		<DropdownMenuContent align="end" className="w-60 text-xs">
			<p className="text-muted-foreground px-2 py-1.5">
				{setLineEnding
					? lineEnding.mixed
						? t("fileStatus.lineEnding.mixed", { ending: lineEnding.ending })
						: t("fileStatus.lineEnding.convert")
					: t("fileStatus.lineEnding.editToConvert")}
			</p>
			{endings.map((item) => (
				<DropdownMenuItem
					key={item.ending}
					className="text-xs"
					disabled={!setLineEnding}
					onSelect={() => setLineEnding?.(item.ending)}
				>
					<span className="flex-1">{item.label}</span>
					{lineEnding.ending === item.ending ? <Check /> : null}
				</DropdownMenuItem>
			))}
		</DropdownMenuContent>
	);
}
