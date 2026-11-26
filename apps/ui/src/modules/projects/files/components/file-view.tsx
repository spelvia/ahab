import { cn } from "cn";
import {
	ChevronRight,
	Code2,
	Columns2,
	Copy,
	Eye,
	ListTree,
	MessageSquarePlus,
	MessagesSquare,
	Pencil,
	Save,
	X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";

import { useEditorSettings } from "@/modules/platform";
import {
	useEditorConfig,
	useExternalFile,
	useFile,
	useSaveFile,
} from "@/modules/projects/files/api";
import { Outline } from "@/modules/projects/files/components/outline";
import {
	type FileEditState,
	fileStatusKey,
	fileStatusStore,
	unsavedStore,
	useFileWatch,
	useMovedAway,
	useUnsaved,
} from "@/modules/projects/files/hooks";
import {
	diskChange,
	type EditorConfig,
	editable,
	resolveIndentation,
} from "@/modules/projects/files/model";
import { problemCode } from "@/shared/api/fetch";
import { explain } from "@/shared/api/problems";
import { AddToChat } from "@/shared/chat-context/add-to-chat";
import { useChatContext } from "@/shared/chat-context/context";
import { linesReferences } from "@/shared/chat-context/model";
import {
	CompareTexts,
	Editor,
	type EditorHandle,
	type SelectionReport,
} from "@/shared/editor/editor";
import { CodeActionsDialog, RenameDialog } from "@/shared/editor/language-ui";
import {
	type CodeActionRequest,
	detectIndentation,
	detectLineEnding,
	type DocumentSymbol,
	encodingOf,
	type LanguageStatus,
	type LineEnding,
	type LineSelection,
	type Location,
	type LocationKind,
	type RenameRequest,
} from "@/shared/editor/model";
import { isExternal } from "@/shared/editor/registry";
import { byteSize } from "@/shared/format";
import { formatNumber, useTranslation } from "@/shared/i18n";
import { mac } from "@/shared/platform";
import {
	fileLanguage,
	type MarkdownView,
	markdownView as savedMarkdownView,
	outlineShown,
	patternLanguages,
	rememberFileLanguage,
	rememberMarkdownView,
	rememberOutlineShown,
	rememberPatternLanguage,
} from "@/shared/preferences/store";
import { type Syntax, useSyntax } from "@/shared/syntax/load";
import { MAX_LINE, patternOf, PLAIN_TEXT, tooLarge } from "@/shared/syntax/model";
import { copyText } from "@/shared/ui/clipboard";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/shared/ui/context-menu";
import { Markdown } from "@/shared/ui/markdown";
import { projectImageUrl } from "@/shared/ui/markdown-images";
import { resolveLink } from "@/shared/ui/markdown-links";
import { Note } from "@/shared/ui/note";
import { toast, toastError } from "@/shared/ui/toast";

/** Markdown files can show rendered. */
function isMarkdown(path: string): boolean {
	return /\.(md|markdown|mdx)$/i.test(path);
}

const VIEWS: { id: MarkdownView; icon: React.ReactNode; wide?: boolean }[] = [
	{ id: "source", icon: <Code2 /> },
	{ id: "preview", icon: <Eye /> },
	{ id: "split", icon: <Columns2 />, wide: true },
];

/** What a window says above its text, when something needs saying. */
type Banner =
	/** A save was refused, or the file changed on disk, while editing. */
	| { kind: "changed"; refused: boolean }
	| { kind: "deleted" }
	/** Unsaved changes kept from before were brought back. */
	| { kind: "restored" };

const COMMAND = mac ? "⌘" : "Ctrl+";

/**
 * A file's window: its text in the editor, read-only until the
 * Edit switch (⌘E), selectable and searchable, with Add to chat for
 * selected lines. Saving (⌘S) goes only over the version the edit started
 * from; a file changed meanwhile says so and offers Compare, Reload and
 * Save anyway. Changes on disk show by themselves when there is nothing
 * unsaved. Markdown shows its source, its rendering, or both.
 */
/** A file's window: one of the project's, or one outside it a language server pointed at. */
export function FileView(props: Parameters<typeof ProjectFileView>[0]) {
	return isExternal(props.path) ? <ExternalFileView {...props} /> : <ProjectFileView {...props} />;
}

/**
 * A file outside the project: read-only, marked with where it
 * comes from, served only by the grant its language server gave.
 */
function ExternalFileView({
	projectId,
	path,
	line,
	jump,
	onOpenLocation,
}: Parameters<typeof ProjectFileView>[0]) {
	const { t } = useTranslation("projects");
	const file = useExternalFile(projectId, path);
	const settings = useEditorSettings(projectId);
	const [locations, setLocations] = useState<Location[] | null>(null);
	const [languageStatus, setLanguageStatus] = useState<LanguageStatus | null>(null);
	const editor = useRef<EditorHandle>(null);
	const key = fileStatusKey(projectId, path);
	const text = file.data?.text;
	const { tabSize, useTabs } = settings.look;
	// The status bar: read-only, outside the project.
	useEffect(() => {
		if (text === undefined) return;
		fileStatusStore.set(key, {
			indentation: { tabSize, useTabs: Boolean(useTabs), source: "settings" },
			encoding: encodingOf(text),
			lineEnding: detectLineEnding(text),
			languageName: languageStatus?.title,
			languageStatus,
			edit: "external",
			goToLine: () => editor.current?.goToLine(),
			goToProblem: (direction) => void editor.current?.goToProblem(direction),
		});
	}, [key, text, tabSize, useTabs, languageStatus]);
	useEffect(() => () => fileStatusStore.remove(key), [key]);
	if (file.isPending) return <Note>{t("state.loading", { ns: "common" })}</Note>;
	if (file.isError || !file.data) return <Note>{t("fileView.external.gone")}</Note>;
	const found = file.data;
	return (
		<div className="bg-background flex h-full min-h-0 flex-col">
			<div className="border-border text-muted-foreground flex h-7 shrink-0 items-center gap-2 border-b px-4 text-xs">
				<span className="text-foreground">{found.name}</span>
				<span>· {found.origin}</span>
				<span className="ml-auto truncate font-mono" title={found.display}>
					{t("fileView.external.readOnly")}
				</span>
			</div>
			{locations ? (
				<ul className="border-border bg-card max-h-32 shrink-0 overflow-auto border-b px-3 py-1.5 font-mono text-xs">
					{locations.map((place) => (
						<li key={`${place.path}:${place.line}`}>
							<button
								type="button"
								className="hover:underline"
								onClick={() => onOpenLocation?.(place.path, place.line)}
							>
								{place.name ?? place.path}:{place.line}
							</button>
						</li>
					))}
				</ul>
			) : null}
			<div className="relative min-h-0 flex-1">
				<Editor
					ref={editor}
					projectId={projectId}
					path={path}
					initial={found.text}
					readOnly
					settings={settings.look}
					onCursor={(cursor) => fileStatusStore.cursor(key, cursor)}
					line={line}
					jump={jump}
					onLocations={(list) => {
						const [first] = list;
						if (list.length === 1 && first) onOpenLocation?.(first.path, first.line);
						else setLocations(list);
					}}
					onLanguageStatus={setLanguageStatus}
				/>
			</div>
		</div>
	);
}

function ProjectFileView({
	projectId,
	projectName,
	path,
	anchor,
	line,
	jump,
	onOpenFile,
	onOpenLocation,
	readOnlyProject = false,
}: {
	projectId: string;
	projectName: string;
	path: string;
	/** A heading to scroll to (markdown), as a link asked. */
	anchor?: string;
	/** A line to show and mark, as search asked. */
	line?: number;
	/** Changes with every request to go to `anchor` or `line`, even the same again. */
	jump?: number;
	/** A link in the file leads to another file of the project. */
	onOpenFile?: (path: string, anchor?: string) => void;
	/** Go to definition led to a place in a file. */
	onOpenLocation?: (path: string, line?: number) => void;
	/** The project is read-only: no Edit switch. */
	readOnlyProject?: boolean;
}) {
	const { t } = useTranslation("projects");
	const dirty = useUnsaved(projectId, path);
	const svg = /\.svg$/i.test(path);
	const [source, setSource] = useState(false);
	const image = !line && ((svg && !source) || /\.(png|jpe?g|gif|webp|avif|bmp|ico)$/i.test(path));
	const [failed, setFailed] = useState<string>();
	const file = useFile(projectId, path, !image);
	const markdown = isMarkdown(path);
	const [chosen, setView] = useState(savedMarkdownView);
	// Asked to show a line: markdown shows its source, where lines are, until
	// another view is chosen (each new request to go to a line asks again).
	const [freedAt, setFreedAt] = useState<number>();
	const pinned = Boolean(line && markdown) && freedAt !== (jump ?? 0);
	const view = pinned ? "source" : chosen;
	const choose = (next: MarkdownView) => {
		setView(next);
		rememberMarkdownView(next);
		setFreedAt(jump ?? 0);
	};
	// Each part keeps the path up to it, which is unique.
	const parts = path
		.split("/")
		.filter(Boolean)
		.map((part, index, all) => ({ part, key: all.slice(0, index + 1).join("/") }));

	const settings = useEditorSettings(projectId);
	const indentation = useEditorConfig(projectId, path).data;
	const data = file.data;
	const canEdit = data ? editable(data, readOnlyProject) : false;

	return (
		<div className="bg-background flex h-full min-h-0 flex-col">
			<nav
				aria-label={t("fileView.path")}
				className="text-muted-foreground flex h-7 shrink-0 items-center gap-1 overflow-hidden px-4 text-xs whitespace-nowrap"
			>
				<span>{projectName}</span>
				{parts.map(({ part, key }, index) => (
					<span key={key} className="flex items-center gap-1">
						<ChevronRight className="size-3" />
						<span className={index === parts.length - 1 ? "text-foreground" : undefined}>
							{part}
						</span>
					</span>
				))}
				{svg && !line ? (
					<button
						type="button"
						className="hover:text-foreground ml-auto"
						disabled={dirty}
						onClick={() => setSource(!source)}
					>
						{t(source ? "fileView.views.preview" : "fileView.views.source")}
					</button>
				) : null}
				{markdown ? (
					<div
						role="radiogroup"
						aria-label={t("fileView.showAs")}
						className="ml-auto flex items-center gap-0.5"
					>
						{VIEWS.map((item) => (
							<button
								key={item.id}
								type="button"
								role="radio"
								aria-checked={view === item.id}
								aria-label={t(`fileView.views.${item.id}`)}
								title={t(`fileView.views.${item.id}`)}
								onClick={() => choose(item.id)}
								className={cn(
									"focus-visible:ring-ring grid size-6 place-items-center rounded outline-none focus-visible:ring-2 [&_svg]:size-3.5",
									view === item.id
										? "bg-accent text-foreground"
										: "text-muted-foreground hover:text-foreground",
									item.wide && "max-md:hidden",
								)}
							>
								{item.icon}
							</button>
						))}
					</div>
				) : null}
			</nav>
			{image ? (
				failed === path ? (
					<Note>{t("fileView.imageFailed")}</Note>
				) : (
					<div className="grid min-h-0 flex-1 place-items-center overflow-auto p-4">
						<img
							alt={path}
							src={projectImageUrl(projectId, path)}
							onError={() => setFailed(path)}
							className="max-h-full max-w-full object-contain"
						/>
					</div>
				)
			) : file.isPending ? (
				<Note>{t("state.loading", { ns: "common" })}</Note>
			) : file.isError ? (
				<Note>
					{explain(
						file.error,
						t("fileView.cannotShow"),
						Object.fromEntries(FILE_PROBLEMS.map((code) => [code, t(`fileView.problems.${code}`)])),
					)}
				</Note>
			) : !data || data.binary ? (
				<Note>{t("fileView.notText", { size: byteSize(data?.size ?? 0) })}</Note>
			) : (
				<FileText
					// A new file, or one read again after it was gone: a new editor.
					key={`${projectId}:${path}`}
					projectId={projectId}
					path={path}
					content={data.content ?? ""}
					version={data.version ?? null}
					truncated={Boolean(data.truncated)}
					size={data.size}
					canEdit={canEdit}
					readOnlyProject={readOnlyProject}
					editableByDefault={settings.behaviour.editableByDefault}
					settingsLoaded={settings.loaded}
					look={settings.look}
					editorconfig={indentation}
					autoSave={settings.behaviour.autoSave}
					autoSaveDelay={settings.behaviour.autoSaveDelay}
					markdown={markdown ? view : undefined}
					anchor={anchor}
					line={line}
					jump={jump}
					refetch={async () => {
						const again = await file.refetch();
						return again.data;
					}}
					onOpenFile={onOpenFile}
					onOpenLocation={onOpenLocation}
				/>
			)}
		</div>
	);
}

interface Disk {
	content: string;
	version: string | null;
}

function FileText({
	projectId,
	path,
	content,
	version,
	truncated,
	size,
	canEdit,
	readOnlyProject,
	editableByDefault,
	settingsLoaded,
	look: settingsLook,
	editorconfig,
	autoSave,
	autoSaveDelay,
	markdown,
	anchor,
	line,
	jump,
	refetch,
	onOpenFile,
	onOpenLocation,
}: {
	projectId: string;
	path: string;
	content: string;
	version: string | null;
	truncated: boolean;
	size: number;
	canEdit: boolean;
	readOnlyProject: boolean;
	editableByDefault: boolean;
	settingsLoaded: boolean;
	look: ReturnType<typeof useEditorSettings>["look"];
	/** What EditorConfig says of the file's indentation. */
	editorconfig?: EditorConfig;
	autoSave: ReturnType<typeof useEditorSettings>["behaviour"]["autoSave"];
	autoSaveDelay: number;
	/** How a markdown file shows; none for other files. */
	markdown?: MarkdownView;
	anchor?: string;
	line?: number;
	jump?: number;
	refetch: () => Promise<{ content?: string | null; version?: string | null } | undefined>;
	onOpenFile?: (path: string, anchor?: string) => void;
	onOpenLocation?: (path: string, line?: number) => void;
}) {
	const { t } = useTranslation("projects");
	const editor = useRef<EditorHandle>(null);
	const save = useSaveFile();
	const chat = useChatContext();
	// Unsaved changes kept from before, brought back once.
	const [kept] = useState(() => (canEdit ? unsavedStore.get(projectId, path) : undefined));
	// What the text started from on disk: what a save goes over. Changes
	// kept from an older version say so when saved.
	const base = useRef<Disk>({
		content,
		version: kept && kept.text !== content ? kept.version : version,
	});
	const [dirty, setDirty] = useState(Boolean(kept && kept.text !== content));
	const [banner, setBanner] = useState<Banner | null>(() =>
		kept && kept.text !== content
			? kept.version === version
				? { kind: "restored" }
				: { kind: "changed", refused: false }
			: null,
	);
	const [chosenEditing, setEditing] = useState<boolean | undefined>(
		kept && kept.text !== content ? true : undefined,
	);
	const editing = canEdit && (chosenEditing ?? (settingsLoaded && editableByDefault));
	const [comparing, setComparing] = useState<Disk | null>(null);
	const [selection, setSelection] = useState<SelectionReport | null>(null);
	const [locations, setLocations] = useState<{ list: Location[]; kind: LocationKind } | null>(null);
	// Language tooling: what serves the file, renames and fixes.
	const [languageStatus, setLanguageStatus] = useState<LanguageStatus | null>(null);
	const [renaming, setRenaming] = useState<RenameRequest | null>(null);
	const [fixing, setFixing] = useState<CodeActionRequest | null>(null);
	const [text, setText] = useState(kept && kept.text !== content ? kept.text : content);

	// The status bar's facts (VS Code's): line endings and indentation found
	// in the text, either changed for this window.
	const [lineEnding, setLineEnding] = useState(() => detectLineEnding(text));
	const detectedIndentation = useMemo(() => detectIndentation(content), [content]);
	const [chosenIndentation, setChosenIndentation] = useState<{
		useTabs: boolean;
		tabSize: number;
	} | null>(null);
	const indentation = resolveIndentation({
		settings: settingsLook,
		editorconfig,
		detected: detectedIndentation,
		chosen: chosenIndentation,
	});
	const look = useMemo(
		() => ({ ...settingsLook, tabSize: indentation.tabSize, useTabs: indentation.useTabs }),
		[settingsLook, indentation.tabSize, indentation.useTabs],
	);

	// The language: chosen for this window, for files like it, or found.
	const syntax = useSyntax();
	const languages = useLanguages(syntax);
	const [chosenLanguage, setChosenLanguage] = useState(() => fileLanguage(projectId, path));
	const pattern = patternOf(path);
	const [patternLanguage, setPatternLanguage] = useState(() => patternLanguages()[pattern]);
	const large = useMemo(() => tooLarge(content), [content]);
	const plainNote = truncated
		? t("fileView.plain.truncated")
		: large
			? t("fileView.plain.tooLarge", { max: formatNumber(MAX_LINE) })
			: undefined;
	const explicitLanguage = plainNote ? PLAIN_TEXT : (chosenLanguage ?? patternLanguage);
	// Found once per text from disk and per change of the languages known.
	const detected = useMemo(
		() => syntax?.detect(path, content),
		// oxlint-disable-next-line react-hooks/exhaustive-deps
		[syntax, path, content, languages, patternLanguage],
	);
	const shownLanguage =
		explicitLanguage && explicitLanguage !== PLAIN_TEXT
			? languages.find((language) => language.id === explicitLanguage)
			: explicitLanguage
				? undefined
				: detected;
	const chooseLanguage = (language: string | undefined) => {
		setChosenLanguage(language);
		rememberFileLanguage(projectId, path, language);
		if (outlineOpen) refreshOutline(600);
	};
	const choosePatternLanguage = (language: string | undefined) => {
		setPatternLanguage(language);
		rememberPatternLanguage(pattern, language);
		// The window follows the pattern from now on.
		chooseLanguage(undefined);
	};

	// The outline: definitions from the syntax tree, kept up to date.
	const [outlineOpen, setOutlineOpen] = useState(outlineShown);
	const [outlineFocus, setOutlineFocus] = useState(0);
	const [symbols, setSymbols] = useState<DocumentSymbol[] | null | undefined>(undefined);
	const outlined = Boolean(shownLanguage?.queries.tags);
	const outlineTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
	const outlineRequest = useRef(0);
	/** Asks the editor for the outline after `delay`, waiting for the editor to load. */
	const refreshOutline = (delay = 400, tries = 20) => {
		clearTimeout(outlineTimer.current);
		outlineTimer.current = setTimeout(() => {
			const current = editor.current;
			if (!current) {
				if (tries > 0) refreshOutline(400, tries - 1);
				return;
			}
			const request = ++outlineRequest.current;
			void current.symbols().then((found) => {
				if (outlineRequest.current === request) setSymbols(found);
			});
		}, delay);
	};
	const latestRefresh = useRef(refreshOutline);
	useEffect(() => {
		latestRefresh.current = refreshOutline;
	});
	useEffect(() => {
		const timer = outlineTimer;
		if (outlineOpen) latestRefresh.current(150);
		return () => clearTimeout(timer.current);
	}, [outlineOpen, latestRefresh, outlineTimer]);
	const toggleOutline = (open: boolean) => {
		setOutlineOpen(open);
		rememberOutlineShown(open);
	};

	const keep = useRef<ReturnType<typeof setTimeout>>(undefined);
	const changed = (next: string) => {
		setText(next);
		const isDirty = next !== base.current.content;
		setDirty(isDirty);
		clearTimeout(keep.current);
		keep.current = setTimeout(() => {
			if (isDirty)
				unsavedStore.keep(projectId, path, {
					text: next,
					version: base.current.version,
					at: Date.now(),
				});
			else unsavedStore.forget(projectId, path);
		}, 300);
	};
	// A window closed right after typing still keeps what was typed.
	// Moved away (renamed, or dragged in the tree): what is not kept yet is
	// kept now, for the window at its new path.
	useMovedAway(projectId, path, () => {
		clearTimeout(keep.current);
		const now = editor.current?.text() ?? text;
		if (now !== base.current.content)
			unsavedStore.keep(projectId, path, {
				text: now,
				version: base.current.version,
				at: Date.now(),
			});
	});

	/** Shows the file as it is on disk, dropping unsaved changes. */
	const load = (disk: Disk) => {
		base.current = disk;
		setLineEnding(detectLineEnding(disk.content));
		editor.current?.replace(disk.content);
		setText(disk.content);
		setDirty(false);
		setBanner(null);
		setComparing(null);
		clearTimeout(keep.current);
		unsavedStore.forget(projectId, path);
	};

	const reload = async () => {
		const fresh = await refetch();
		if (fresh) load({ content: fresh.content ?? "", version: fresh.version ?? null });
	};

	const saving = useRef(false);
	/** Saves as asked (⌘S, Save): formatted first when the project says so. */
	const saveAsked = async () => {
		const current = editor.current;
		if (editing && current?.formatOnSave()) await current.format().catch(() => false);
		doSave();
	};
	const doSave = (anyway = false) => {
		if (!canEdit || saving.current) return;
		const current = editor.current?.text() ?? text;
		if (!anyway && current === base.current.content && base.current.version !== null) return;
		saving.current = true;
		save.mutate(
			{
				id: projectId,
				params: { path },
				data: {
					content: current,
					expectedVersion: anyway ? null : base.current.version,
				},
			},
			{
				onSuccess: (saved) => {
					base.current = { content: current, version: saved.version };
					editor.current?.saved();
					const now = editor.current?.text() ?? current;
					setDirty(now !== current);
					setBanner(null);
					setComparing(null);
					if (now === current) {
						clearTimeout(keep.current);
						unsavedStore.forget(projectId, path);
					}
				},
				onError: (error) => {
					if (problemCode(error) === "file_changed") setBanner({ kind: "changed", refused: true });
					else toastError(error);
				},
				onSettled: () => {
					saving.current = false;
				},
			},
		);
	};

	// The file changed on disk (an agent, Git, another device).
	useFileWatch(projectId, path, Boolean(version) && !truncated, (disk) => {
		switch (diskChange(base.current.version, dirty, disk)) {
			case "same":
				return;
			case "reload":
				void reload();
				return;
			case "conflict":
				setBanner({ kind: "changed", refused: false });
				return;
			case "deleted":
				setBanner({ kind: "deleted" });
		}
	});

	// Auto-save, as Settings → Editor says: a moment after typing stops.
	const blocked = banner?.kind === "changed" || banner?.kind === "deleted";
	const autoSaving = useRef<ReturnType<typeof setTimeout>>(undefined);
	const latestSave = useRef(doSave);
	useEffect(() => {
		latestSave.current = doSave;
	});
	const typed = () => {
		clearTimeout(autoSaving.current);
		if (!editing || blocked || autoSave !== "afterDelay") return;
		autoSaving.current = setTimeout(() => latestSave.current(), autoSaveDelay);
	};

	const references = (lines: LineSelection[]) =>
		linesReferences(projectId, path, dirty ? null : base.current.version, lines);
	const selected = () => editor.current?.selections() ?? selection?.lines ?? [];

	const compare = async () => {
		const fresh = await refetch();
		setComparing({ content: fresh?.content ?? "", version: fresh?.version ?? null });
	};

	// The status bar shows the focused window's facts, and acts on them.
	const statusKey = fileStatusKey(projectId, path);
	const acts = useRef({
		toggleEdit: () => {},
		chooseLanguage: chooseLanguage,
		choosePatternLanguage: choosePatternLanguage,
		convert: (_ending: LineEnding) => {},
	});
	useEffect(() => {
		acts.current = {
			toggleEdit: () => {
				if (canEdit) setEditing(!editing);
			},
			chooseLanguage,
			choosePatternLanguage,
			convert: (ending) => {
				if (!editing) return;
				setLineEnding({ ending, mixed: false });
				// The editor reports the text with the new endings once it renders again.
				setTimeout(() => {
					const now = editor.current?.text();
					if (now !== undefined) changed(now);
				}, 0);
			},
		};
	});
	const editState: FileEditState = editing
		? "editing"
		: truncated
			? "truncated"
			: readOnlyProject
				? "readOnlyProject"
				: canEdit
					? "viewing"
					: "readOnly";
	const encoding = encodingOf(content);
	useEffect(() => {
		fileStatusStore.set(statusKey, {
			indentation,
			encoding,
			lineEnding,
			language: {
				languages,
				detected,
				chosen: chosenLanguage,
				pattern,
				patternChosen: Boolean(patternLanguage),
				note: plainNote,
				choose: (language) => acts.current.chooseLanguage(language),
				choosePattern: (language) => acts.current.choosePatternLanguage(language),
			},
			languageStatus,
			edit: editState,
			toggleEdit: canEdit ? () => acts.current.toggleEdit() : undefined,
			goToLine: () => editor.current?.goToLine(),
			goToProblem: (direction) => void editor.current?.goToProblem(direction),
			setIndentation: setChosenIndentation,
			setLineEnding: editing ? (ending) => acts.current.convert(ending) : undefined,
		});
		// oxlint-disable-next-line react-hooks/exhaustive-deps
	}, [
		statusKey,
		indentation.useTabs,
		indentation.tabSize,
		indentation.source,
		encoding,
		lineEnding,
		languages,
		detected,
		chosenLanguage,
		pattern,
		patternLanguage,
		plainNote,
		languageStatus,
		editState,
		canEdit,
		editing,
	]);
	useEffect(() => () => fileStatusStore.remove(statusKey), [statusKey]);

	const editorElement = (fill: boolean) => (
		<ContextMenu>
			<ContextMenuTrigger asChild>
				<div className={cn("relative", fill ? "min-h-0 flex-1" : undefined)}>
					<Editor
						ref={editor}
						projectId={projectId}
						path={path}
						initial={text}
						readOnly={!editing}
						settings={look}
						language={explicitLanguage}
						onOutline={() => {
							toggleOutline(true);
							setOutlineFocus((count) => count + 1);
						}}
						line={line}
						jump={jump}
						fill={fill}
						onChange={(next) => {
							changed(next);
							typed();
							if (outlineOpen) refreshOutline();
						}}
						onSelect={setSelection}
						onSave={() => (editing ? void saveAsked() : undefined)}
						onToggleEdit={() => {
							if (canEdit) setEditing(!editing);
						}}
						onAddToChat={chat ? (lines) => chat.add(references(lines)) : undefined}
						onChooseChat={chat ? (lines) => chat.choose(references(lines)) : undefined}
						onLocations={(list, kind) => {
							const [first] = list;
							if (list.length === 1 && first && !first.approximate)
								onOpenLocation?.(first.path, first.line);
							else setLocations({ list, kind });
						}}
						onLanguageStatus={setLanguageStatus}
						lineEnding={lineEnding.ending}
						onCursor={(cursor) => fileStatusStore.cursor(statusKey, cursor)}
						onRename={setRenaming}
						onCodeActions={setFixing}
						onBlur={() => {
							if (editing && dirty && !blocked && autoSave === "onFocusChange") doSave();
						}}
					/>
					{selection && chat ? (
						<div className="absolute z-10" style={{ top: selection.top, left: selection.left }}>
							<AddToChat references={() => references(selected())} />
						</div>
					) : null}
				</div>
			</ContextMenuTrigger>
			<ContextMenuContent>
				{chat ? (
					<>
						<ContextMenuItem
							disabled={!selection}
							onSelect={() => chat.add(references(selected()))}
						>
							<MessageSquarePlus />
							{chat.target
								? t("fileView.menu.addToTarget", { title: chat.target.title })
								: t("fileView.menu.addToChat")}
							<span className="text-muted-foreground ml-auto pl-4 text-xs">{COMMAND}L</span>
						</ContextMenuItem>
						<ContextMenuItem
							disabled={!selection}
							onSelect={() => chat.choose(references(selected()))}
						>
							<MessagesSquare />
							{t("fileView.menu.addToChat")}
						</ContextMenuItem>
						<ContextMenuSeparator />
					</>
				) : null}
				<LanguageItems editor={editor} editing={editing} />
				<ContextMenuItem
					disabled={!selection}
					onSelect={() =>
						void copyText(
							selected()
								.map((item) => item.text)
								.join("\n"),
						).then((copied) =>
							toast(copied ? t("fileView.menu.copied") : t("fileView.menu.cannotCopy")),
						)
					}
				>
					<Copy />
					{t("fileView.menu.copyLines")}
				</ContextMenuItem>
			</ContextMenuContent>
		</ContextMenu>
	);

	return (
		<>
			<div className="border-border flex min-h-8 shrink-0 flex-wrap items-center gap-2 border-b px-3 py-1 text-xs">
				{truncated ? (
					<span className="text-muted-foreground">
						{t("fileView.toolbar.truncated", { size: byteSize(size) })}
					</span>
				) : readOnlyProject ? (
					<span className="text-muted-foreground">{t("fileView.toolbar.readOnlyProject")}</span>
				) : !canEdit ? (
					<span className="text-muted-foreground">{t("fileView.toolbar.readOnly")}</span>
				) : (
					<button
						type="button"
						aria-pressed={editing}
						title={t(editing ? "fileView.toolbar.stopEditingTitle" : "fileView.toolbar.editTitle", {
							shortcut: `${COMMAND}E`,
						})}
						onClick={() => setEditing(!editing)}
						className={cn(
							"focus-visible:ring-ring flex h-6 items-center gap-1.5 rounded-md px-2 outline-none focus-visible:ring-2",
							editing
								? "bg-accent text-foreground"
								: "text-muted-foreground hover:text-foreground hover:bg-accent/60",
						)}
					>
						<Pencil className="size-3.5" />
						{editing ? t("fileView.toolbar.editing") : t("fileView.toolbar.edit")}
					</button>
				)}
				{dirty ? (
					<span className="text-warning" title={t("fileView.toolbar.unsavedTitle")}>
						{t("fileView.toolbar.unsaved")}
					</span>
				) : null}
				<span className="ml-auto" />
				{markdown && markdown !== "source" ? null : (
					<button
						type="button"
						aria-pressed={outlineOpen}
						title={t("fileView.toolbar.outlineTitle", { shortcut: mac ? "⇧⌘O" : "Ctrl+Shift+O" })}
						onClick={() => toggleOutline(!outlineOpen)}
						className={cn(
							"focus-visible:ring-ring flex h-6 items-center gap-1.5 rounded-md px-2 outline-none focus-visible:ring-2 max-md:hidden",
							outlineOpen
								? "bg-accent text-foreground"
								: "text-muted-foreground hover:text-foreground hover:bg-accent/60",
						)}
					>
						<ListTree className="size-3.5" />
						{t("fileView.toolbar.outline")}
					</button>
				)}
				{editing ? (
					<button
						type="button"
						disabled={!dirty || save.isPending}
						onClick={() => void saveAsked()}
						title={t("fileView.toolbar.saveTitle", { shortcut: `${COMMAND}S` })}
						className="bg-primary text-primary-foreground focus-visible:ring-ring flex h-6 items-center gap-1.5 rounded-md px-2 font-medium outline-none focus-visible:ring-2 disabled:opacity-40"
					>
						<Save className="size-3.5" />
						{save.isPending
							? t("actions.saving", { ns: "common" })
							: t("actions.save", { ns: "common" })}
					</button>
				) : null}
			</div>
			{banner ? (
				<BannerView
					banner={banner}
					comparing={Boolean(comparing)}
					onCompare={() => void (comparing ? setComparing(null) : compare())}
					onReload={() => void reload()}
					onSaveAnyway={() => doSave(true)}
					onDismiss={() => setBanner(null)}
					onDiscard={() => void reload()}
				/>
			) : null}
			{locations ? (
				<div className="border-border bg-card shrink-0 border-b px-3 py-1.5 text-xs">
					<div className="mb-1 flex items-center">
						<span className="text-muted-foreground">
							{t(
								locations.list.some((place) => place.approximate)
									? "fileView.locations.approximate"
									: "fileView.locations.places",
								{ count: locations.list.length },
							)}
						</span>
						<button
							type="button"
							aria-label={t("actions.close", { ns: "common" })}
							onClick={() => setLocations(null)}
							className="text-muted-foreground hover:text-foreground ml-auto"
						>
							<X className="size-3.5" />
						</button>
					</div>
					<ul className="max-h-32 overflow-auto font-mono">
						{locations.list.map((place) => (
							<li key={`${place.path}:${place.line}:${place.character ?? 0}`}>
								<button
									type="button"
									className="hover:underline"
									onClick={() => onOpenLocation?.(place.path, place.line)}
								>
									{place.name ?? place.path}:{place.line}
									{place.origin ? (
										<span className="text-muted-foreground"> · {place.origin}</span>
									) : null}
								</button>
							</li>
						))}
					</ul>
				</div>
			) : null}
			{comparing ? (
				<CompareTexts
					left={comparing.content}
					right={text}
					leftLabel={t("fileView.compare.onDisk")}
					rightLabel={t("fileView.compare.yours")}
				/>
			) : markdown && markdown !== "source" ? (
				<MarkdownFile
					projectId={projectId}
					text={text}
					path={path}
					split={markdown === "split"}
					anchor={anchor}
					jump={jump}
					onOpenFile={onOpenFile}
					source={editorElement(true)}
				/>
			) : outlineOpen ? (
				<div className="flex min-h-0 flex-1">
					<div className="flex min-h-0 min-w-0 flex-1 flex-col">{editorElement(true)}</div>
					<Outline
						symbols={outlined ? symbols : null}
						focusFilter={outlineFocus}
						onReveal={(symbol) => editor.current?.reveal(symbol.nameFrom, symbol.nameTo)}
					/>
				</div>
			) : (
				editorElement(true)
			)}
			{renaming ? <RenameDialog request={renaming} onClose={() => setRenaming(null)} /> : null}
			{fixing ? <CodeActionsDialog request={fixing} onClose={() => setFixing(null)} /> : null}
		</>
	);
}

/** The context menu's language tooling: navigation, and edits in Edit mode. */
function LanguageItems({
	editor,
	editing,
}: {
	editor: React.RefObject<EditorHandle | null>;
	editing: boolean;
}) {
	const { t } = useTranslation("projects");
	const current = editor.current;
	if (!current?.offers("definition")) return null;
	const items: {
		label: LanguageItem;
		key?: string;
		run: () => void;
		edit?: boolean;
		show: boolean;
	}[] = [
		{
			label: "definition",
			key: "F12",
			run: () => current.navigate("definition"),
			show: true,
		},
		{
			label: "typeDefinition",
			run: () => current.navigate("typeDefinition"),
			show: current.offers("typeDefinition"),
		},
		{
			label: "implementation",
			run: () => current.navigate("implementation"),
			show: current.offers("implementation"),
		},
		{
			label: "references",
			key: "⇧F12",
			run: () => current.navigate("references"),
			show: current.offers("references"),
		},
		{
			label: "rename",
			key: "F2",
			run: () => current.rename(),
			edit: true,
			show: current.offers("rename"),
		},
		{
			label: "fixes",
			key: mac ? "⌘." : "Ctrl+.",
			run: () => current.codeActions(),
			edit: true,
			show: current.offers("codeActions"),
		},
		{
			label: "format",
			key: mac ? "⇧⌥F" : "Shift+Alt+F",
			run: () => void current.format(),
			edit: true,
			show: current.offers("format"),
		},
	];
	return (
		<>
			{items
				.filter((item) => item.show && (!item.edit || editing))
				.map((item) => (
					<ContextMenuItem key={item.label} onSelect={item.run}>
						{t(`fileView.language.${item.label}`)}
						{item.key ? (
							<span className="text-muted-foreground ml-auto pl-4 text-xs">{item.key}</span>
						) : null}
					</ContextMenuItem>
				))}
			<ContextMenuSeparator />
		</>
	);
}

const NO_LANGUAGES: ReturnType<Syntax["languages"]> = [];

/** The languages known, kept up to date as plugins add theirs. */
function useLanguages(syntax: Syntax | undefined) {
	return useSyncExternalStore(
		(listener) => syntax?.onLanguagesChanged(listener) ?? (() => undefined),
		() => syntax?.languages() ?? NO_LANGUAGES,
	);
}

function BannerView({
	banner,
	comparing,
	onCompare,
	onReload,
	onSaveAnyway,
	onDismiss,
	onDiscard,
}: {
	banner: Banner;
	comparing: boolean;
	onCompare: () => void;
	onReload: () => void;
	onSaveAnyway: () => void;
	onDismiss: () => void;
	onDiscard: () => void;
}) {
	const { t } = useTranslation("projects");
	const button =
		"focus-visible:ring-ring rounded-md border border-current/30 px-2 py-0.5 font-medium outline-none hover:bg-current/10 focus-visible:ring-2";
	return (
		<div
			role={banner.kind === "restored" ? "status" : "alert"}
			className={cn(
				"flex shrink-0 flex-wrap items-center gap-2 border-b px-3 py-1.5 text-xs",
				banner.kind === "restored"
					? "border-border bg-card text-foreground"
					: "border-warning/40 bg-warning/10 text-warning",
			)}
		>
			{banner.kind === "changed" ? (
				<>
					<span className="mr-auto">
						{banner.refused ? t("fileView.banner.refused") : t("fileView.banner.changed")}
					</span>
					<button type="button" className={button} onClick={onCompare}>
						{comparing ? t("fileView.banner.backToEditing") : t("fileView.banner.compare")}
					</button>
					<button type="button" className={button} onClick={onReload}>
						{t("fileView.banner.reload")}
					</button>
					<button type="button" className={button} onClick={onSaveAnyway}>
						{t("fileView.banner.saveAnyway")}
					</button>
					{!banner.refused ? (
						<button type="button" className={button} onClick={onDismiss}>
							{t("fileView.banner.keepMine")}
						</button>
					) : null}
				</>
			) : banner.kind === "deleted" ? (
				<>
					<span className="mr-auto">{t("fileView.banner.deleted")}</span>
					<button type="button" className={button} onClick={onSaveAnyway}>
						{t("fileView.banner.saveAgain")}
					</button>
					<button type="button" className={button} onClick={onDismiss}>
						{t("fileView.banner.dismiss")}
					</button>
				</>
			) : (
				<>
					<span className="mr-auto">{t("fileView.banner.restored")}</span>
					<button type="button" className={button} onClick={onDiscard}>
						{t("fileView.banner.discard")}
					</button>
					<button type="button" className={button} onClick={onDismiss}>
						{t("fileView.banner.ok")}
					</button>
				</>
			)}
		</div>
	);
}

/**
 * Markdown rendered, alone or beside its source (on wide screens). Side by
 * side, scrolling one scrolls the other to the same place, proportionally.
 */
function MarkdownFile({
	projectId,
	text,
	path,
	split,
	anchor,
	jump,
	onOpenFile,
	source,
}: {
	projectId: string;
	text: string;
	path: string;
	split: boolean;
	anchor?: string;
	jump?: number;
	onOpenFile?: (path: string, anchor?: string) => void;
	/** The source, in the editor. */
	source: React.ReactNode;
}) {
	const preview = useRef<HTMLDivElement>(null);
	const scrollTo = (id: string) =>
		preview.current?.querySelector(`[id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "start" });
	// Opened at a heading (a link from another file): go there, again for
	// each request (`jump`), once the text is there.
	const target = anchor && text ? `${jump ?? 0}#${anchor}` : undefined;
	useEffect(() => {
		if (!target) return;
		const id = target.slice(target.indexOf("#") + 1);
		preview.current?.querySelector(`[id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "start" });
	}, [target]);
	// Links to files open them; links to headings scroll; others leave.
	const follow = (href: string) => {
		const link = resolveLink(path, href);
		if (!link) return false;
		if (link.path) onOpenFile?.(link.path, link.anchor);
		else if (link.anchor) scrollTo(link.anchor);
		return true;
	};
	// The pane being scrolled by the other, so it does not answer back.
	const following = useRef<Element | null>(null);
	const sourceScroller = useRef<Element | null>(null);
	const keepUp = (from: Element | null, to: Element | null) => {
		if (!from || !to || following.current === from) {
			following.current = null;
			return;
		}
		const range = from.scrollHeight - from.clientHeight;
		const ratio = range > 0 ? from.scrollTop / range : 0;
		following.current = to;
		to.scrollTop = ratio * (to.scrollHeight - to.clientHeight);
	};
	const rendered = (
		<div
			ref={preview}
			onScroll={() => split && keepUp(preview.current, sourceScroller.current)}
			className="min-h-0 overflow-auto"
		>
			<div className="mx-auto max-w-3xl px-6 py-5">
				<Markdown html text={text} onLink={follow} imageBase={{ project: projectId, path }} />
			</div>
		</div>
	);
	if (!split) return <div className="flex min-h-0 flex-1 flex-col">{rendered}</div>;
	return (
		<div className="grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)] md:grid-cols-2">
			<div
				// The editor scrolls inside; its scroller is the event's target.
				onScrollCapture={(event) => {
					sourceScroller.current = event.target as Element;
					keepUp(sourceScroller.current, preview.current);
				}}
				className="border-border flex min-h-0 flex-col border-r max-md:hidden"
			>
				{source}
			</div>
			{rendered}
		</div>
	);
}

/** The problems the file view has its own words for (`fileView.problems`). */
const FILE_PROBLEMS = [
	"file_too_large",
	"machine_offline",
	"not_found",
	"permission_denied",
] as const;

/** The context menu's language items, by their key in `fileView.language`. */
type LanguageItem =
	| "definition"
	| "typeDefinition"
	| "implementation"
	| "references"
	| "rename"
	| "fixes"
	| "format";
