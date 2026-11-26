import { cn } from "cn";
import { ChevronRight, Keyboard, Plus, RotateCcw, Search, X } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";

import { useFeatureFlags } from "@/modules/platform/feature";
import { VimMappings } from "@/modules/platform/keyboard/components/vim-mappings";
import { Trans, useTranslation } from "@/shared/i18n";
import {
	ACTIONS,
	type Action,
	type ActionId,
	browserKeeps,
	conflicts,
	keyOf,
	keyParts,
	type Keymap,
	keymapOf,
	shown,
	usable,
} from "@/shared/keymap/model";
import { changedShortcuts, resetShortcuts, setShortcut } from "@/shared/keymap/store";
import { useKeymap } from "@/shared/keymap/use-keymap";
import { appWindow, mac } from "@/shared/platform";
import { Button } from "@/shared/ui/button";

const DEFAULTS = keymapOf({});
/** The groups of actions, with the key of each one's heading. */
const GROUPS = [
	["General", "general"],
	["Windows", "windows"],
	["Files", "files"],
	["Editor", "editor"],
	["Chat", "chat"],
] as const;
const titleOf = (id: ActionId) => ACTIONS.find((action) => action.id === id)?.title ?? id;
/** Whether a key reaches Ahab here: a browser tab keeps some for itself. */
const reaches = (key: string) => appWindow || !browserKeeps(key);

/**
 * Settings → Keyboard: every shortcut, Ahab's and the editor's, changed by
 * pressing keys, and Vim's mappings; kept in this browser.
 */
export function KeyboardPage() {
	const { t } = useTranslation("platform");
	const map = useKeymap();
	const flag = useFeatureFlags();
	const [filter, setFilter] = useState("");
	// Filtering by pressing a key, rather than typing a name.
	const [byKey, setByKey] = useState<string>();
	const [listening, setListening] = useState(false);
	const [open, setOpen] = useState<ActionId>();
	const [status, setStatus] = useState("");
	const clashes = conflicts(map);
	const changed = Object.keys(changedShortcuts()).length;
	const needle = filter.trim().toLowerCase();
	const matches = (action: Action) =>
		byKey
			? map[action.id].includes(byKey)
			: !needle ||
				action.title.toLowerCase().includes(needle) ||
				map[action.id].some((key) => shown(key).toLowerCase().includes(needle));
	const shownActions = ACTIONS.filter(matches);

	useKeyCapture(
		listening,
		(key) => {
			setByKey(key);
			setFilter("");
			setListening(false);
		},
		() => setListening(false),
	);

	return (
		<section className="space-y-6">
			<header className="flex flex-wrap items-start justify-between gap-4">
				<div className="space-y-1">
					<h1 className="text-2xl font-semibold">{t("keyboard.title")}</h1>
					<p className="text-muted-foreground max-w-2xl text-sm text-pretty">
						{t("keyboard.intro")}
					</p>
				</div>
				{changed ? (
					<Button
						size="sm"
						variant="outline"
						onClick={() => {
							resetShortcuts();
							setStatus(t("keyboard.allReset"));
						}}
					>
						<RotateCcw />
						{t("keyboard.resetAll", { count: changed })}
					</Button>
				) : null}
			</header>

			<div className="flex max-w-md items-center gap-2">
				<div className="relative flex-1">
					<Search
						aria-hidden
						className="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2"
					/>
					{byKey ? (
						<div className="border-input bg-background flex h-9 items-center gap-2 rounded-md border pr-1 pl-8">
							<span className="text-muted-foreground text-sm">{t("keyboard.using")}</span>
							<Keys keyName={byKey} />
							<button
								type="button"
								onClick={() => setByKey(undefined)}
								aria-label={t("keyboard.clearKeyFilter")}
								className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring ml-auto grid size-7 place-items-center rounded-md outline-none focus-visible:ring-2"
							>
								<X className="size-3.5" />
							</button>
						</div>
					) : (
						<input
							type="search"
							value={listening ? "" : filter}
							readOnly={listening}
							onChange={(event) => setFilter(event.target.value)}
							placeholder={
								listening ? t("keyboard.filterListening") : t("keyboard.filterPlaceholder")
							}
							aria-label={t("keyboard.filterLabel")}
							className={cn(
								"border-input bg-background focus-visible:ring-ring/40 h-9 w-full rounded-md border pr-3 pl-8 text-sm outline-none focus-visible:ring-3",
								listening && "border-primary placeholder:text-primary",
							)}
						/>
					)}
				</div>
				<Button
					size="icon"
					variant={listening ? "secondary" : "outline"}
					aria-pressed={listening}
					aria-label={t("keyboard.findByKeys")}
					title={t("keyboard.findByKeys")}
					onClick={() => setListening((on) => !on)}
				>
					<Keyboard />
				</Button>
			</div>

			<p role="status" aria-live="polite" className="text-muted-foreground -mt-3 h-4 text-xs">
				{status}
			</p>

			{shownActions.length === 0 ? (
				<div className="border-border rounded-[10px] border border-dashed px-4 py-8 text-center">
					<p className="text-heading text-sm font-semibold">{t("keyboard.noMatch.title")}</p>
					<p className="text-muted-foreground mt-1 text-sm">
						{byKey ? (
							<Trans
								t={t}
								i18nKey="keyboard.noMatch.keyFree"
								components={{ key: <Keys keyName={byKey} /> }}
							/>
						) : (
							t("keyboard.noMatch.tryAnother")
						)}
					</p>
				</div>
			) : null}

			{GROUPS.map(([group, heading]) => {
				const actions = shownActions.filter((action) => action.group === group);
				if (actions.length === 0) return null;
				return (
					<section key={group} aria-labelledby={`keys-${group}`} className="space-y-2 pt-2">
						<h2 id={`keys-${group}`} className="text-heading text-base font-bold">
							{t(`keyboard.groups.${heading}`)}
						</h2>
						{group === "Editor" || group === "Chat" ? (
							<p className="text-muted-foreground max-w-2xl text-sm text-pretty">
								{t(group === "Editor" ? "keyboard.editorNote" : "keyboard.chatNote")}
							</p>
						) : null}
						<ul className="border-border divide-border divide-y border-y">
							{actions.map((action) => (
								<ShortcutRow
									key={action.id}
									action={action}
									map={map}
									clashes={clashes}
									open={open === action.id}
									onOpen={(next) => setOpen(next ? action.id : undefined)}
									onStatus={setStatus}
									off={
										action.feature && !flag(action.feature)
											? t(`keyboard.featureNames.${action.feature}`)
											: undefined
									}
								/>
							))}
						</ul>
					</section>
				);
			})}

			<VimMappings />

			<p className="text-muted-foreground max-w-2xl text-xs text-pretty">
				{mac ? t("keyboard.terminalNoteMac") : t("keyboard.terminalNote")}{" "}
				{appWindow ? null : t("keyboard.browserNote")}
			</p>
		</section>
	);
}

/**
 * While `on`, takes the next key pressed (with its modifiers) before anything
 * else sees it; Escape alone cancels. `onHeld` hears the modifiers as they
 * are held, before the key.
 */
function useKeyCapture(
	on: boolean,
	onKey: (key: string) => void,
	onCancel: () => void,
	onHeld?: (parts: string[]) => void,
) {
	const handlers = useRef({ onKey, onCancel, onHeld });
	useEffect(() => {
		handlers.current = { onKey, onCancel, onHeld };
	});
	useEffect(() => {
		if (!on) return;
		const held = (event: KeyboardEvent) =>
			handlers.current.onHeld?.(
				[
					event.ctrlKey && "Ctrl",
					event.altKey && "Alt",
					event.shiftKey && "Shift",
					event.metaKey && "Meta",
				].filter(Boolean) as string[],
			);
		const down = (event: KeyboardEvent) => {
			event.preventDefault();
			event.stopPropagation();
			const plain = !event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey;
			if (plain && event.code === "Escape") return handlers.current.onCancel();
			const key = keyOf(event);
			if (key) handlers.current.onKey(key);
			else held(event);
		};
		const up = (event: KeyboardEvent) => held(event);
		window.addEventListener("keydown", down, true);
		window.addEventListener("keyup", up, true);
		return () => {
			window.removeEventListener("keydown", down, true);
			window.removeEventListener("keyup", up, true);
		};
	}, [on]);
}

/** A key as keycaps: one cap per modifier and the key. */
function Keys({
	keyName,
	quiet,
	className,
}: {
	keyName: string;
	quiet?: boolean;
	className?: string;
}) {
	return (
		<kbd
			aria-label={shown(keyName)}
			className={cn("inline-flex items-center gap-0.5 font-sans", className)}
		>
			{keyParts(keyName).map((part) => (
				<Cap key={part} quiet={quiet}>
					{part}
				</Cap>
			))}
		</kbd>
	);
}

function Cap({ children, quiet }: { children: React.ReactNode; quiet?: boolean }) {
	return (
		<span
			aria-hidden
			className={cn(
				"border-border inline-flex h-6 min-w-6 items-center justify-center rounded-md border border-b-2 px-1.5 text-xs leading-none font-medium",
				quiet ? "text-muted-foreground bg-transparent" : "bg-background text-heading",
			)}
		>
			{children}
		</span>
	);
}

function ShortcutRow({
	action,
	map,
	clashes,
	open,
	onOpen,
	onStatus,
	off,
}: {
	action: Action;
	map: Keymap;
	clashes: Map<string, ActionId[]>;
	open: boolean;
	onOpen: (open: boolean) => void;
	onStatus: (message: string) => void;
	/** The feature it belongs to, when that is off. */
	off?: string;
}) {
	const { t } = useTranslation("platform");
	const keys = map[action.id];
	const changed = action.id in changedShortcuts();
	const clashing = keys.some((key) => (clashes.get(key)?.length ?? 0) > 1);
	// The key to lead with: the first that reaches Ahab here.
	const lead = keys.find(reaches) ?? keys[0];
	const rest = keys.filter((key) => key !== lead);
	const editor = useId();

	return (
		<li className={cn(open && "bg-card")}>
			<button
				type="button"
				aria-expanded={open}
				aria-controls={editor}
				onClick={() => onOpen(!open)}
				className={cn(
					"focus-visible:ring-ring flex w-full flex-wrap items-center gap-x-4 gap-y-2 px-3 py-3 text-left transition-colors duration-150 outline-none focus-visible:ring-2 focus-visible:ring-inset",
					!open && "hover:bg-accent/40",
				)}
			>
				<span className="flex min-w-0 flex-1 basis-48 items-center gap-2">
					<ChevronRight
						aria-hidden
						className={cn(
							"text-muted-foreground size-4 shrink-0 transition-transform duration-150",
							open && "rotate-90",
						)}
					/>
					<span
						className={cn("text-sm font-semibold", off ? "text-muted-foreground" : "text-heading")}
					>
						{action.title}
					</span>
					{changed ? <span className="pill pill-muted">{t("keyboard.row.changed")}</span> : null}
					{off ? (
						<span className="pill pill-muted">
							{t("keyboard.row.featureOff", { feature: off })}
						</span>
					) : null}
					{clashing ? <span className="pill pill-danger">{t("keyboard.row.clash")}</span> : null}
				</span>
				<span className={cn("flex flex-wrap items-center justify-end gap-2", off && "opacity-60")}>
					{lead ? (
						<Keys keyName={lead} />
					) : (
						<span className="text-muted-foreground text-sm">{t("keyboard.row.noShortcut")}</span>
					)}
					{rest.length ? (
						<span className="text-muted-foreground text-xs">
							{t("keyboard.row.more", { count: rest.length })}
						</span>
					) : null}
				</span>
			</button>
			{open ? (
				<ShortcutEditor
					id={editor}
					action={action}
					map={map}
					clashes={clashes}
					changed={changed}
					onClose={() => onOpen(false)}
					onStatus={onStatus}
				/>
			) : null}
		</li>
	);
}

type Recording =
	| { state: "idle" }
	| { state: "listening"; held: string[] }
	/** A key another action has: moving it needs a yes. */
	| { state: "taken"; key: string; from: ActionId[] }
	| { state: "refused"; key: string; reason: string };

function ShortcutEditor({
	id,
	action,
	map,
	clashes,
	changed,
	onClose,
	onStatus,
}: {
	id: string;
	action: Action;
	map: Keymap;
	clashes: Map<string, ActionId[]>;
	changed: boolean;
	onClose: () => void;
	onStatus: (message: string) => void;
}) {
	const { t } = useTranslation("platform");
	const { t: tc } = useTranslation();
	const keys = map[action.id];
	const [recording, setRecording] = useState<Recording>({ state: "idle" });
	const listening = recording.state === "listening";

	const add = (key: string) => {
		setShortcut(action.id, [...keys, key]);
		onStatus(t("keyboard.editor.added", { key: shown(key), action: action.title }));
		setRecording({ state: "idle" });
	};

	useKeyCapture(
		listening,
		(key) => {
			if (keys.includes(key))
				return setRecording({
					state: "refused",
					key,
					reason: t("keyboard.editor.hasItAlready", { action: action.title }),
				});
			if (!usable(key))
				return setRecording({
					state: "refused",
					key,
					reason: mac ? t("keyboard.editor.needsModifierMac") : t("keyboard.editor.needsModifier"),
				});
			const from = ACTIONS.filter(
				(other) => other.id !== action.id && map[other.id].includes(key),
			).map((other) => other.id);
			if (from.length) return setRecording({ state: "taken", key, from });
			add(key);
		},
		() => setRecording({ state: "idle" }),
		(held) => setRecording({ state: "listening", held }),
	);

	// Escape closes the editor when nothing is being recorded.
	useEffect(() => {
		if (listening) return;
		const onKey = (event: KeyboardEvent) => {
			if (event.key !== "Escape") return;
			if (recording.state !== "idle") setRecording({ state: "idle" });
			else onClose();
		};
		window.addEventListener("keydown", onKey);
		return () => window.removeEventListener("keydown", onKey);
	}, [listening, recording.state, onClose]);

	return (
		<div id={id} className="space-y-3 px-3 pt-1 pb-4 pl-9">
			{keys.length ? (
				<ul
					aria-label={t("keyboard.editor.keys", { action: action.title })}
					className="max-w-xl space-y-1"
				>
					{keys.map((key) => {
						const others = (clashes.get(key) ?? []).filter((other) => other !== action.id);
						return (
							<li key={key} className="flex flex-wrap items-center gap-x-3 gap-y-1">
								<Keys keyName={key} quiet={!reaches(key)} />
								<span className="text-muted-foreground min-w-0 flex-1 text-xs">
									{others.length ? (
										<span className="text-destructive">
											{t("keyboard.editor.also", { actions: others.map(titleOf).join(", ") })}
										</span>
									) : reaches(key) ? null : (
										t("keyboard.editor.appOnly")
									)}
								</span>
								<Button
									size="xs"
									variant="ghost"
									aria-label={t("keyboard.editor.remove", {
										key: shown(key),
										action: action.title,
									})}
									onClick={() => {
										setShortcut(
											action.id,
											keys.filter((other) => other !== key),
										);
										onStatus(
											t("keyboard.editor.removed", { key: shown(key), action: action.title }),
										);
									}}
								>
									<X />
									{tc("actions.remove")}
								</Button>
							</li>
						);
					})}
				</ul>
			) : (
				<p className="text-muted-foreground text-sm">{t("keyboard.editor.noKeys")}</p>
			)}

			{recording.state === "idle" ? (
				<Button
					size="sm"
					variant="outline"
					onClick={() => setRecording({ state: "listening", held: [] })}
				>
					<Plus />
					{t("keyboard.editor.addKey")}
				</Button>
			) : recording.state === "listening" ? (
				<div
					role="status"
					className="border-primary bg-background flex h-9 max-w-sm items-center gap-2 rounded-md border px-3"
				>
					{recording.held.length ? (
						<span className="flex items-center gap-0.5">
							{keyParts(`${recording.held.join("+")}+`)
								.slice(0, -1)
								.map((part) => (
									<Cap key={part}>{part}</Cap>
								))}
							<span className="text-muted-foreground ml-1 text-sm">…</span>
						</span>
					) : (
						<span className="text-primary text-sm">{t("keyboard.editor.pressKeys")}</span>
					)}
					<span className="text-muted-foreground ml-auto text-xs">
						{t("keyboard.editor.escCancels")}
					</span>
				</div>
			) : (
				<div
					role="alert"
					className={cn(
						"max-w-lg space-y-3 rounded-[10px] border p-3",
						recording.state === "taken" ? "border-border bg-background" : "border-destructive/50",
					)}
				>
					<p className="flex flex-wrap items-center gap-2 text-sm">
						<Keys keyName={recording.key} />
						{recording.state === "taken" ? (
							<span>
								<Trans
									t={t}
									i18nKey="keyboard.editor.taken"
									values={{ actions: recording.from.map(titleOf).join(", ") }}
									components={{ action: <span className="text-heading font-semibold" /> }}
								/>
							</span>
						) : (
							<span className="text-destructive">{recording.reason}</span>
						)}
					</p>
					<div className="flex gap-2">
						{recording.state === "taken" ? (
							<Button
								size="sm"
								onClick={() => {
									for (const other of recording.from)
										setShortcut(
											other,
											map[other].filter((key) => key !== recording.key),
										);
									add(recording.key);
								}}
							>
								{t("keyboard.editor.moveHere")}
							</Button>
						) : (
							<Button size="sm" onClick={() => setRecording({ state: "listening", held: [] })}>
								{t("keyboard.editor.tryAnother")}
							</Button>
						)}
						<Button size="sm" variant="ghost" onClick={() => setRecording({ state: "idle" })}>
							{tc("actions.cancel")}
						</Button>
					</div>
				</div>
			)}

			<div className="border-border flex max-w-xl flex-wrap items-center justify-between gap-2 border-t pt-3">
				{changed ? (
					<Button
						size="sm"
						variant="ghost"
						onClick={() => {
							setShortcut(action.id, undefined);
							onStatus(t("keyboard.editor.reset", { action: action.title }));
						}}
					>
						<RotateCcw />
						{t("keyboard.editor.resetToDefaults")}
						<span className="text-muted-foreground font-normal max-sm:hidden">
							{t("keyboard.editor.defaults", {
								keys:
									DEFAULTS[action.id].map((key) => shown(key)).join(", ") ||
									t("keyboard.editor.none"),
							})}
						</span>
					</Button>
				) : (
					<span className="text-muted-foreground text-xs">{t("keyboard.editor.ownKeys")}</span>
				)}
				<Button size="sm" variant="outline" onClick={onClose}>
					{t("keyboard.editor.done")}
				</Button>
			</div>
		</div>
	);
}
