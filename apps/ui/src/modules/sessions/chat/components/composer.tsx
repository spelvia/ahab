import { useQuery } from "@tanstack/react-query";
import { cn } from "cn";
import {
	ArrowUp,
	ChevronUp,
	ListEnd,
	LoaderCircle,
	Paperclip,
	SlidersHorizontal,
	Square,
	Zap,
} from "lucide-react";
import {
	type KeyboardEvent,
	type ReactNode,
	useEffect,
	useLayoutEffect,
	useMemo,
	useDeferredValue,
	useRef,
	useState,
} from "react";

import { type ChatDelivery, useChatSettings } from "@/modules/platform";
import { useFileIndex, rank } from "@/modules/projects";
import { type Attachments, AttachmentList } from "@/modules/sessions/attachment";
import { QueueList } from "@/modules/sessions/chat/components/queue-list";
import type { QueuedMessage } from "@/modules/sessions/chat/model";
import { draftStore, DraftReferences, useDraft } from "@/modules/sessions/reference";
import type { AgentCommand, CommandInvocation } from "@/shared/api/generated/ahab.schemas";
import { onAppearance } from "@/shared/appearance";
import { fileReference, type ContextReference } from "@/shared/chat-context/model";
import { PromptEditor, type PromptEditorHandle } from "@/shared/editor/prompt-editor";
import { useTranslation } from "@/shared/i18n";
import { usePhone } from "@/shared/media";
import { composerDraft, rememberComposerDraft } from "@/shared/preferences/store";
import { PickerPlacement } from "@/shared/ui/picker";
import { Sheet, SheetContent, SheetTrigger } from "@/shared/ui/sheet";

/**
 * Where you write to the agent: Enter sends, Shift+Enter starts a new line.
 * While the agent works, a message is queued until its turn ends or sent by
 * stopping the turn, as Settings → Agent editor says (Alt+Enter: the other way);
 * a queued one can still be sent now. Stop ends the turn. With `attachments`, files are attached with the
 * paperclip or by pasting, and sent with the message. With `draftKey`, the
 * context added to it shows as chips and is sent with the message
 * too. With Vim on (Settings → Agent editor), it has Vim's keys.
 */
export function Composer({
	commandChat,
	projectId,
	commandPlacement = "above",
	readCommands,
	placeholder,
	disabled,
	working,
	pending,
	settings,
	settingsSummary,
	meters,
	onSend,
	onStop,
	stopping,
	awaitingAnswer = false,
	autoFocus,
	attachments,
	draftKey,
	messageDraftKey = draftKey,
	onOpenFile,
	onType,
	queue,
	onUnqueue,
	onSendNow,
	onReorder,
	onEditQueued,
}: {
	placeholder: string;
	commandChat?: string;
	projectId?: string;
	commandPlacement?: "above" | "below";
	readCommands?: (id: string, signal: AbortSignal) => Promise<AgentCommand[]>;
	disabled?: boolean;
	working?: boolean;
	pending?: boolean;
	/**
	 * The agent's settings for the next message (model, effort, mode, fast):
	 * in the toolbar, or on phones one button that opens them in a sheet.
	 */
	settings?: React.ReactNode;
	/** What the settings button says on phones: the model and its effort. */
	settingsSummary?: string;
	/** At the toolbar's end, always shown: the session's usage and plan. */
	meters?: React.ReactNode;
	/** `delivery` is given while the agent works. */
	onSend: (
		text: string,
		attachments: string[],
		references: ContextReference[],
		delivery?: ChatDelivery,
		command?: CommandInvocation,
	) => void | Promise<unknown>;
	onStop?: () => void;
	stopping?: boolean;
	/**
	 * The agent waits for an answer to a request: Escape does not stop the
	 * turn (as in the transcript), the Stop button still does.
	 */
	awaitingAnswer?: boolean;
	autoFocus?: boolean;
	attachments?: Attachments;
	/** Where the composer's context draft is kept: the session's ID, or a New session window's key. */
	draftKey?: string;
	/** Separates message text and attachments when tabs share a context draft. */
	messageDraftKey?: string;
	/** A chip opens its file. */
	onOpenFile?: (path: string, line?: number) => void;
	/** Something was typed (the session becomes the chat target). */
	onType?: () => void;
	/** Messages waiting for the agent's turn to end. */
	queue?: readonly QueuedMessage[];
	onUnqueue?: (id: string) => void;
	/** Sends a queued message next, stopping the turn for it. */
	onSendNow?: (id: string) => void;
	/** Puts the waiting messages in a new order: their IDs, first first. */
	onReorder?: (order: string[]) => void;
	/** Changes a waiting message's text. */
	onEditQueued?: (id: string, text: string) => Promise<unknown>;
}) {
	const { t } = useTranslation("sessions");
	const phone = usePhone();
	const chat = useChatSettings();
	const [text, setText] = useState(() =>
		messageDraftKey ? (composerDraft<string>(messageDraftKey, "text") ?? "") : "",
	);
	const keepText = (value: string) => {
		setText(value);
		if (messageDraftKey) rememberComposerDraft(messageDraftKey, "text", value || undefined);
	};
	const [selected, setSelected] = useState<AgentCommand | undefined>(() =>
		messageDraftKey ? composerDraft<AgentCommand>(messageDraftKey, "command") : undefined,
	);
	const saveSelection = (entry?: AgentCommand) => {
		if (selected?.id !== entry?.id || selected?.kind !== entry?.kind) saveFields({});
		setSelected(entry);
		if (messageDraftKey) rememberComposerDraft(messageDraftKey, "command", entry);
	};
	const [fields, setFields] = useState<Record<string, string>>(() =>
		messageDraftKey
			? (composerDraft<Record<string, string>>(messageDraftKey, "arguments") ?? {})
			: {},
	);
	const saveFields = (values: Record<string, string>) => {
		setFields(values);
		if (messageDraftKey) rememberComposerDraft(messageDraftKey, "arguments", values);
	};
	const hasSlash = Boolean(commandChat && text.startsWith("/"));
	const {
		data: catalog,
		isError: loadError,
		refetch,
	} = useQuery({
		queryKey: ["chatCommands", commandChat],
		enabled: hasSlash && Boolean(readCommands),
		queryFn: ({ signal }) => (commandChat && readCommands ? readCommands(commandChat, signal) : []),
		retry: false,
		staleTime: 30_000,
		refetchOnWindowFocus: false,
	});
	const box = useRef<HTMLTextAreaElement>(null);
	const container = useRef<HTMLDivElement>(null);
	const focusMessage = () =>
		container.current?.querySelector<HTMLElement>("textarea, [contenteditable=true]")?.focus();
	const [active, setActive] = useState(0);
	const [dismissed, setDismissed] = useState(false);
	const token = text.split(/\s/, 1)[0]?.slice(1) ?? "";
	const matches = catalog?.filter((entry) => entry.name === token) ?? [];
	const chosen =
		selected?.name === token ? selected : matches.length === 1 ? matches[0] : undefined;
	const missing =
		selected &&
		selected.name === token &&
		catalog &&
		!matches.some((entry) => entry.kind === selected.kind && entry.id === selected.id);
	const requiredMissing = chosen?.arguments?.some(
		(arg) => arg.required && !fields[arg.name]?.trim(),
	);
	const ambiguous = !chosen && matches.length > 1;
	const [cursor, setCursor] = useState<number | null>(null);
	const editor = useRef<PromptEditorHandle>(null);
	const prefix =
		cursor !== null && projectId && draftKey
			? /(?:^|\s)@([^\s@]*)$/.exec(text.slice(0, cursor))
			: null;
	const fileQuery = prefix?.[1];
	const filesOpen = fileQuery !== undefined && !dismissed;
	const fileIndex = useFileIndex(projectId ?? "", filesOpen);
	const wanted = useDeferredValue(fileQuery);
	const paths = useMemo(
		() =>
			wanted === undefined
				? []
				: wanted
					? rank(wanted, fileIndex.data?.paths ?? [], 24).map((item) => item.path)
					: (fileIndex.data?.paths.slice(0, 24) ?? []),
		[wanted, fileIndex.data],
	);
	const picking =
		!filesOpen && hasSlash && !dismissed && (!/\s/.test(text) || ambiguous || loadError);
	const results =
		catalog?.filter((entry) =>
			[
				entry.name,
				entry.server,
				entry.description,
				entry.kind === "mcpPrompt" ? "mcp" : "skill",
			].some((value) => value?.toLowerCase().includes(token.toLowerCase())),
		) ?? [];
	const moveActive = (next: number) => {
		setActive(next);
		container.current
			?.querySelector(`[data-command-index="${next}"]`)
			?.scrollIntoView?.({ block: "nearest" });
	};
	const choose = (entry: AgentCommand) => {
		saveSelection(entry);
		typed(`/${entry.name} ${text.replace(/^\S+\s*/, "")}`, entry);
		setDismissed(true);
		focusMessage();
	};
	const chooseFile = (path: string) => {
		if (!draftKey || !projectId || fileQuery === undefined || cursor === null) return;
		const from = cursor - fileQuery.length - 1;
		const to = cursor + (/^[^\s@]*/.exec(text.slice(cursor))?.[0].length ?? 0);
		draftStore.add(draftKey, [fileReference(projectId, path)]);
		if (chat.vim) editor.current?.replaceRange(from, to);
		else {
			typed(text.slice(0, from) + text.slice(to));
			requestAnimationFrame(() => box.current?.setSelectionRange(from, from));
		}
		setDismissed(true);
		focusMessage();
	};
	const captureKey = (event: KeyboardEvent<HTMLDivElement>) => {
		if (
			(!picking && !filesOpen) ||
			event.nativeEvent.isComposing ||
			event.target instanceof HTMLButtonElement ||
			event.target instanceof HTMLInputElement
		)
			return;
		const count = filesOpen ? paths.length : results.length;
		if (event.key === "Escape") setDismissed(true);
		else if (event.key === "ArrowDown" && count) moveActive((active + 1) % count);
		else if (event.key === "ArrowUp" && count) moveActive((active - 1 + count) % count);
		else if (event.key === "Enter" && filesOpen) {
			if (paths[active] && wanted === fileQuery) chooseFile(paths[active]);
		} else if (event.key === "Enter" && results[active]) choose(results[active]);
		else return;
		event.preventDefault();
		event.stopPropagation();
	};
	const draftEdit = useRef(0);
	const submitting = useRef(false);
	const delivery = chat.whileWorking;
	const picker = useRef<HTMLInputElement>(null);
	const ids = attachments?.ids ?? [];
	const references = useDraft(draftKey);
	const waiting = attachments?.uploading ?? false;
	const ready =
		(text.trim() !== "" || ids.length > 0 || references.length > 0) &&
		!waiting &&
		!(hasSlash && text.trim() === "/") &&
		!ambiguous &&
		!missing &&
		!requiredMissing &&
		(!hasSlash || catalog !== undefined);
	const send = async (how: ChatDelivery = delivery) => {
		if (!ready || disabled || pending || submitting.current) return;
		submitting.current = true;
		const edit = draftEdit.current;
		try {
			const args = [text.trim(), ids, references, working ? how : undefined] as const;
			if (chosen) {
				const values: Record<string, string> = {};
				for (const arg of chosen.arguments ?? []) {
					const value = fields[arg.name];
					if (value) values[arg.name] = value;
				}
				await onSend(...args, {
					id: chosen.id,
					kind: chosen.kind,
					...(Object.keys(values).length && { arguments: values }),
				});
			} else await onSend(...args);
			if (draftEdit.current === edit) {
				keepText("");
				saveSelection();
				setDismissed(false);
			}
			for (const file of attachments?.files ?? []) {
				if (file.attachment) attachments?.remove(file.key);
			}
		} catch {
			// The caller shows the failure. Keep the draft available to edit or retry.
		} finally {
			submitting.current = false;
		}
	};
	const other: ChatDelivery = delivery === "queue" ? "interrupt" : "queue";
	// A touch screen's Enter is a new line (its keyboard has no Shift+Enter);
	// the send button sends, or Ctrl/⌘+Enter with a keyboard attached.
	const [touch] = useState(
		() =>
			typeof window !== "undefined" && Boolean(window.matchMedia?.("(pointer: coarse)").matches),
	);
	const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
		if (
			event.key === "Escape" &&
			!event.nativeEvent.isComposing &&
			working &&
			onStop &&
			!stopping &&
			!awaitingAnswer
		) {
			event.preventDefault();
			event.stopPropagation();
			onStop();
			return;
		}
		if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
		if (touch && !event.metaKey && !event.ctrlKey) return;
		event.preventDefault();
		// Alt+Enter: the other way, for this message.
		void send(event.altKey ? other : delivery);
	};
	// Measure only when the text, available width or font changes.
	const width = useRef(0);
	useLayoutEffect(() => {
		if (!chat.vim) fit(box.current, Boolean(text));
	}, [chat.vim, text, box]);
	useEffect(() => {
		const element = box.current;
		if (chat.vim || !element) return;
		const appearance = onAppearance(() => fit(element));
		width.current = element.clientWidth;
		const observer =
			typeof ResizeObserver === "undefined"
				? undefined
				: new ResizeObserver(() => {
						if (width.current === element.clientWidth) return;
						width.current = element.clientWidth;
						fit(element);
					});
		observer?.observe(element);
		return () => {
			observer?.disconnect();
			appearance();
		};
	}, [chat.vim, box]);
	const typed = (value: string, entry = selected) => {
		if (value.split(/\s/, 1)[0] !== `/${entry?.name}`) saveSelection();
		setDismissed(false);
		setActive(0);
		draftEdit.current++;
		keepText(value);
		onType?.();
	};
	const pasteFiles = (files: File[]) => {
		if (files.length && attachments?.enabled) attachments.add(files);
	};
	const hint = disabled
		? placeholder
		: projectId && draftKey
			? t(commandChat ? "composer.entryHint" : "composer.fileHint", { message: placeholder })
			: placeholder;
	const sendLabel = working
		? t(delivery === "queue" ? "composer.sendQueued" : "composer.sendInterrupt", {
				other: t(other === "queue" ? "composer.queue" : "composer.interrupt"),
			})
		: t("composer.sendTitle");
	return (
		<div
			data-chat-composer
			ref={container}
			className="relative flex flex-col gap-1.5"
			onKeyDownCapture={captureKey}
		>
			{picking || filesOpen ? (
				<div
					className={cn(
						"bg-popover text-popover-foreground border-border z-20 max-h-[min(36dvh,18rem)] overflow-y-auto rounded-md border p-1",
						commandPlacement === "below" ? "order-last" : "absolute inset-x-0 bottom-full mb-1",
					)}
					aria-label={t(filesOpen ? "composer.files" : "composer.commands")}
				>
					{filesOpen
						? paths.map((path, i) => (
								<button
									type="button"
									key={path}
									data-command-index={i}
									title={path}
									aria-label={path}
									aria-current={active === i || undefined}
									onMouseDown={(event) => event.preventDefault()}
									onClick={() => chooseFile(path)}
									disabled={wanted !== fileQuery}
									className={cn(
										"flex min-h-10 w-full items-center gap-2 rounded px-2 text-left text-sm sm:min-h-8",
										active === i && "bg-accent",
									)}
								>
									<span className="max-w-[60%] shrink-0 truncate">{path.split("/").pop()}</span>
									<span className="text-muted-foreground truncate text-xs">
										{path.slice(0, Math.max(0, path.lastIndexOf("/")))}
									</span>
								</button>
							))
						: null}
					{filesOpen && !paths.length && (
						<div role="status" className="text-muted-foreground px-2 py-1.5 text-sm">
							{fileIndex.isError ? (
								<button type="button" onClick={() => void fileIndex.refetch()}>
									{t("composer.retryFiles")}
								</button>
							) : (
								t(fileIndex.isPending ? "composer.loadingFiles" : "composer.noFiles")
							)}
						</div>
					)}
					{!filesOpen &&
						results.map((entry, index) => (
							<button
								key={entry.kind + entry.id}
								data-command-index={index}
								type="button"
								onMouseDown={(event) => event.preventDefault()}
								onClick={() => choose(entry)}
								className={cn(
									"flex min-h-10 w-full items-baseline gap-2 rounded px-2 py-1 text-left text-sm sm:min-h-8 hover:bg-accent focus-visible:ring-2",
									index === active && "bg-accent",
								)}
							>
								<span className="max-w-[70%] shrink-0 truncate font-medium">
									/{entry.name}{" "}
									<span className="text-muted-foreground font-normal">
										{entry.server ?? t("composer.skill")}
									</span>
								</span>
								{entry.description && (
									<span className="text-muted-foreground truncate text-xs">
										{entry.description}
									</span>
								)}
								{!entry.server &&
									results.some(
										(candidate) => candidate !== entry && candidate.name === entry.name,
									) && <span className="text-muted-foreground text-xs break-all">{entry.id}</span>}
							</button>
						))}
					{!filesOpen && !results.length && (
						<div className="p-2 text-sm" role="status">
							{loadError ? (
								<button type="button" onClick={() => void refetch()}>
									{t("composer.retryCommands")}
								</button>
							) : (
								t(catalog ? "composer.noCommands" : "composer.loadingCommands")
							)}
						</div>
					)}
				</div>
			) : null}
			{queue?.length ? (
				<QueueList
					queue={queue}
					working={working}
					onUnqueue={onUnqueue}
					onSendNow={onSendNow}
					onReorder={onReorder}
					onEdit={onEditQueued}
				/>
			) : null}
			<div
				className={cn(
					"border-input bg-card focus-within:ring-ring/40 relative overflow-hidden rounded-xl border p-2.5 focus-within:ring-2",
					working && "border-primary/50",
				)}
			>
				{working ? <WorkingBar /> : null}
				{missing && (
					<p role="status" className="text-warning px-2 text-sm">
						{t("composer.commandGone")}
					</p>
				)}
				{chosen?.kind === "mcpPrompt" && !missing && (
					<fieldset className="mb-2 grid max-h-[25dvh] gap-1.5 overflow-y-auto px-1">
						<legend className="text-muted-foreground text-xs">
							{t("composer.promptFrom", { server: chosen.server })}
						</legend>
						{chosen.arguments?.map((arg) => (
							<label className="grid gap-1 text-xs" key={arg.name}>
								<span>
									{arg.name}
									{arg.required ? " *" : ""}
								</span>
								<input
									className="border-input bg-background min-h-10 rounded-md border px-2 text-base sm:min-h-8 sm:text-sm"
									aria-required={arg.required}
									placeholder={arg.description ?? undefined}
									value={fields[arg.name] ?? ""}
									onInput={(event) => {
										draftEdit.current++;
										saveFields({ ...fields, [arg.name]: event.currentTarget.value });
									}}
								/>
							</label>
						))}
					</fieldset>
				)}
				{draftKey ? <DraftReferences draftKey={draftKey} onOpenFile={onOpenFile} /> : null}
				{attachments ? <AttachmentList attachments={attachments} /> : null}
				{chat.vim ? (
					<PromptEditor
						ref={editor}
						onCursor={setCursor}
						value={text}
						label={t("composer.message")}
						placeholder={hint}
						disabled={disabled}
						autoFocus={autoFocus}
						onChange={typed}
						enterSends={!touch}
						onSubmit={(alt) => void send(alt ? other : delivery)}
						onPasteFiles={attachments?.enabled ? pasteFiles : undefined}
					/>
				) : (
					<textarea
						ref={box}
						aria-label={t("composer.message")}
						// oxlint-disable-next-line jsx-a11y/no-autofocus
						autoFocus={autoFocus}
						value={text}
						disabled={disabled}
						onSelect={(event) =>
							setCursor(
								event.currentTarget.selectionStart === event.currentTarget.selectionEnd
									? event.currentTarget.selectionStart
									: null,
							)
						}
						onChange={(event) => {
							setCursor(event.target.selectionStart);
							typed(event.target.value);
						}}
						onKeyDown={onKeyDown}
						onPaste={(event) => {
							const files = [...event.clipboardData.files];
							if (files.length === 0 || !attachments?.enabled) return;
							event.preventDefault();
							attachments.add(files);
						}}
						placeholder={hint}
						rows={2}
						enterKeyHint={touch ? "enter" : "send"}
						className="placeholder:text-muted-foreground max-h-[40vh] min-h-12 w-full resize-none overflow-y-auto bg-transparent px-1.5 py-1 text-sm outline-none disabled:opacity-60 md:max-h-80"
					/>
				)}
				<div className="flex items-end gap-2">
					<div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-1 gap-y-0.5 max-sm:flex-nowrap">
						{attachments?.enabled ? (
							<>
								<button
									type="button"
									onClick={() => picker.current?.click()}
									disabled={disabled}
									aria-label={t("composer.attach")}
									title={t("composer.attachTitle")}
									className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-11 shrink-0 place-items-center rounded-md outline-none focus-visible:ring-2 disabled:opacity-40 sm:size-7"
								>
									<Paperclip className="size-4" />
								</button>
								<input
									ref={picker}
									type="file"
									multiple
									hidden
									onChange={(event) => {
										attachments.add([...(event.target.files ?? [])]);
										event.target.value = "";
									}}
								/>
							</>
						) : null}
						{settings && phone ? (
							<AgentSettingsSheet summary={settingsSummary}>{settings}</AgentSettingsSheet>
						) : (
							settings
						)}
						{meters ? (
							<span className="ml-auto flex shrink-0 items-center gap-1">{meters}</span>
						) : null}
					</div>
					{working && onStop ? (
						<button
							type="button"
							onClick={onStop}
							disabled={stopping}
							aria-label={t(stopping ? "tools.stopping" : "composer.stop")}
							title={t(stopping ? "tools.stopping" : "composer.stop")}
							className="bg-foreground text-background focus-visible:ring-ring grid size-11 shrink-0 place-items-center rounded-xl outline-none focus-visible:ring-2 sm:size-8 sm:rounded-lg"
						>
							{stopping ? (
								<LoaderCircle className="size-3.5 motion-safe:animate-spin" />
							) : (
								<Square className="size-3.5 fill-current" />
							)}
						</button>
					) : null}
					{!working || !onStop || ready ? (
						<button
							type="button"
							onClick={() => void send()}
							disabled={!ready || disabled || pending}
							aria-label={working ? sendLabel : t("composer.send")}
							title={waiting ? t("composer.uploading") : sendLabel}
							className="bg-primary text-primary-foreground focus-visible:ring-ring grid size-11 shrink-0 place-items-center rounded-xl outline-none focus-visible:ring-2 disabled:opacity-40 sm:size-8 sm:rounded-lg"
						>
							{working && delivery === "queue" ? (
								<ListEnd className="size-4" />
							) : working ? (
								<Zap className="size-4" />
							) : (
								<ArrowUp className="size-4" />
							)}
						</button>
					) : null}
				</div>
			</div>
		</div>
	);
}

/**
 * On phones, the agent's settings behind one button, so the toolbar needs no
 * sideways scrolling: it says the model, and opens a sheet that lists each
 * setting as a full-width row.
 */
function AgentSettingsSheet({ summary, children }: { summary?: string; children: ReactNode }) {
	const { t } = useTranslation("sessions");
	return (
		<Sheet>
			<SheetTrigger
				aria-label={
					summary ? t("composer.agentSettingsOf", { summary }) : t("composer.agentSettings")
				}
				className="text-muted-foreground hover:text-foreground hover:bg-accent data-[state=open]:bg-accent focus-visible:ring-ring flex h-11 min-w-0 flex-1 items-center gap-1.5 rounded-md px-2 text-left text-sm outline-none focus-visible:ring-2"
			>
				<SlidersHorizontal className="size-4 shrink-0" aria-hidden />
				<span className="min-w-0 truncate">{summary || t("composer.agentSettings")}</span>
				<ChevronUp className="size-3.5 shrink-0 opacity-60" aria-hidden />
			</SheetTrigger>
			<SheetContent title={t("composer.agentSettings")}>
				<PickerPlacement value="sheet">
					<div className="flex min-h-0 flex-col gap-0.5 overflow-y-auto px-2 pb-2 [&>span]:mx-3 [&>span]:my-1.5 [&>span]:self-start">
						{children}
					</div>
				</PickerPlacement>
			</SheetContent>
		</Sheet>
	);
}

function fit(element: HTMLTextAreaElement | null, hasText = Boolean(element?.value)) {
	if (!element) return;
	element.style.height = "";
	if (hasText) element.style.height = `${element.scrollHeight}px`;
}

/** Along the composer's top edge while the agent works. */
function WorkingBar() {
	return (
		<div aria-hidden className="bg-primary/15 absolute inset-x-0 top-0 h-0.5 overflow-hidden">
			<div className="loading-bar bg-primary h-full w-1/3 rounded-full" />
		</div>
	);
}

/** The messages waiting for the agent's turn to end, each to send now or take back. */
