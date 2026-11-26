import "@xterm/xterm/css/xterm.css";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal as Xterm } from "@xterm/xterm";
import { ChevronDown, ChevronUp, RotateCcw, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { useRestartTerminal } from "@/modules/sessions/terminal/api";
import { ctrlOf, findPaths, forAhab } from "@/modules/sessions/terminal/model";
import type { Terminal } from "@/shared/api/generated/ahab.schemas";
import { onAppearance, textScale } from "@/shared/appearance";
import { AddToChat } from "@/shared/chat-context/add-to-chat";
import { useChatContext } from "@/shared/chat-context/context";
import { referenceId } from "@/shared/chat-context/model";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { IconButton } from "@/shared/ui/icon-button";
import { toastError } from "@/shared/ui/toast";

/** The app's colours, for the terminal: its background, text and accents. */
function theme() {
	const style = getComputedStyle(document.documentElement);
	const value = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
	// A colour theme's eight colours (Settings → Appearance); xterm's own otherwise.
	const ansi = (name: string) => style.getPropertyValue(`--ansi-${name}`).trim() || undefined;
	const colors = {
		black: ansi("black"),
		red: ansi("red"),
		green: ansi("green"),
		yellow: ansi("yellow"),
		blue: ansi("blue"),
		magenta: ansi("magenta"),
		cyan: ansi("cyan"),
		white: ansi("white"),
	};
	return {
		background: value("--background", "#252526"),
		foreground: value("--foreground", "#d4d4d4"),
		cursor: value("--primary", "#8fb3a3"),
		cursorAccent: value("--background", "#252526"),
		selectionBackground: `${value("--primary", "#8fb3a3")}55`,
		...colors,
		brightBlack: colors.black,
		brightRed: colors.red,
		brightGreen: colors.green,
		brightYellow: colors.yellow,
		brightBlue: colors.blue,
		brightMagenta: colors.magenta,
		brightCyan: colors.cyan,
		brightWhite: colors.white,
	};
}

/** Code's font (Settings → Appearance), for the terminal. */
function codeFont(): string {
	return (
		getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim() ||
		'"JetBrains Mono Variable", "JetBrains Mono", ui-monospace, monospace'
	);
}

/** Where a terminal's attachment WebSocket is. */
function address(id: string): string {
	const scheme = window.location.protocol === "https:" ? "wss" : "ws";
	return `${scheme}://${window.location.host}/api/v1/terminals/${id}/attach`;
}

type Link = "connecting" | "open" | "reconnecting";

/**
 * A terminal window: xterm.js attached to the terminal on its
 * machine. Every window showing it sees the same screen and can type; the
 * terminal takes the size of the one used last. Closing the window never
 * stops the terminal.
 */
export function TerminalView({
	terminal,
	root,
	onOpenFile,
}: {
	terminal: Terminal;
	/** The project's folder: absolute paths inside it are links too. */
	root?: string;
	/** Opens a file of the project at a line (a path in the output, clicked). */
	onOpenFile?: (path: string, line?: number) => void;
}) {
	const { t } = useTranslation(["sessions", "common"]);
	const host = useRef<HTMLDivElement>(null);
	// What the parts outside xterm (search, the key row) reach it by.
	const handle = useRef<{ send: (data: string) => void; search?: SearchAddon; xterm?: Xterm }>({
		send: () => {},
	});
	const [searching, setSearching] = useState(false);
	// Selected output, offered to a chat.
	const [selected, setSelected] = useState("");
	const chat = useChatContext();
	const [query, setQuery] = useState("");
	const [ctrl, setCtrl] = useState(false);
	const ctrlArmed = useRef(false);
	const touch = typeof window !== "undefined" && window.matchMedia?.("(pointer: coarse)").matches;
	// The newest callbacks, for xterm's handlers made once.
	const opener = useRef({ root, onOpenFile });
	useEffect(() => {
		opener.current = { root, onOpenFile };
	}, [root, onOpenFile]);
	const [link, setLink] = useState<Link>("connecting");
	const [exited, setExited] = useState<number | null | undefined>(
		terminal.state === "exited" ? (terminal.exitCode ?? null) : undefined,
	);
	const restart = useRestartTerminal();
	const id = terminal.id;

	useEffect(() => {
		const element = host.current;
		if (!element) return;
		const xterm = new Xterm({
			fontFamily: codeFont(),
			fontSize: Math.round(13 * textScale()),
			lineHeight: 1.15,
			cursorBlink: true,
			scrollback: 10_000,
			allowProposedApi: true,
			theme: theme(),
		});
		const fit = new FitAddon();
		xterm.loadAddon(fit);
		const search = new SearchAddon();
		xterm.loadAddon(search);
		handle.current.search = search;
		handle.current.xterm = xterm;
		const selecting = xterm.onSelectionChange(() => setSelected(xterm.getSelection()));
		// Paths into the project open its files (`src/main.rs:42`).
		const links = xterm.registerLinkProvider({
			provideLinks(row, callback) {
				const text = xterm.buffer.active.getLine(row - 1)?.translateToString(true) ?? "";
				const found = findPaths(text, opener.current.root);
				callback(
					found.map((path) => ({
						range: { start: { x: path.start + 1, y: row }, end: { x: path.end, y: row } },
						text: text.slice(path.start, path.end),
						activate: () => opener.current.onOpenFile?.(path.path, path.line),
					})),
				);
			},
		});
		// ⌘/Ctrl+F searches the scrollback instead of the page; Ahab's own
		// shortcuts pass the terminal by (it leaves them to the page).
		xterm.attachCustomKeyEventHandler((event) => {
			if (
				event.type === "keydown" &&
				(event.metaKey || event.ctrlKey) &&
				!event.shiftKey &&
				event.key.toLowerCase() === "f"
			) {
				event.preventDefault();
				setSearching(true);
				return false;
			}
			return !forAhab(event);
		});
		xterm.loadAddon(
			new WebLinksAddon((_event, uri) => window.open(uri, "_blank", "noopener,noreferrer")),
		);
		xterm.open(element);
		try {
			const webgl = new WebglAddon();
			webgl.onContextLoss(() => webgl.dispose());
			xterm.loadAddon(webgl);
		} catch {
			// Without WebGL, xterm draws with the DOM.
		}
		fit.fit();

		// What this attachment is at: its socket, its retry, whether it was
		// stopped, and whether the process ended (output after that is a
		// restarted one's).
		const at: {
			socket?: WebSocket;
			retry?: ReturnType<typeof setTimeout>;
			stopped: boolean;
			ended: boolean;
		} = { stopped: false, ended: false };
		const encoder = new TextEncoder();
		// This window's size: sent when it is used, so the terminal follows it.
		const resize = () => {
			if (at.socket?.readyState === WebSocket.OPEN)
				at.socket.send(JSON.stringify({ type: "resize", cols: xterm.cols, rows: xterm.rows }));
		};
		const connect = () => {
			if (at.stopped) return;
			const opened = new WebSocket(address(id));
			opened.binaryType = "arraybuffer";
			at.socket = opened;
			opened.addEventListener("open", () => {
				setLink("open");
				resize();
			});
			opened.addEventListener("message", (message) => {
				if (message.data instanceof ArrayBuffer) {
					if (at.ended) {
						at.ended = false;
						setExited(undefined);
					}
					xterm.write(new Uint8Array(message.data));
					return;
				}
				const event = JSON.parse(String(message.data)) as {
					type: string;
					code?: number;
				};
				if (event.type === "reset") {
					xterm.reset();
					at.ended = false;
				} else if (event.type === "exited") {
					at.ended = true;
					setExited(event.code ?? null);
				}
			});
			opened.addEventListener("close", () => {
				if (at.stopped) return;
				setLink("reconnecting");
				at.retry = setTimeout(connect, 1_500);
			});
		};
		connect();

		const send = (data: string) => {
			if (at.socket?.readyState === WebSocket.OPEN) at.socket.send(encoder.encode(data));
		};
		handle.current.send = send;
		const typing = xterm.onData((data) => {
			// The key row's Ctrl applies to the next key typed.
			if (ctrlArmed.current && data.length === 1) {
				ctrlArmed.current = false;
				setCtrl(false);
				send(ctrlOf(data));
				return;
			}
			send(data);
		});
		const binary = xterm.onBinary((data) => {
			if (at.socket?.readyState !== WebSocket.OPEN) return;
			at.socket.send(Uint8Array.from(data, (char) => char.charCodeAt(0)));
		});
		const observer = new ResizeObserver(() => {
			fit.fit();
			resize();
		});
		observer.observe(element);
		// Using this window makes it the terminal's size again.
		const focused = () => resize();
		element.addEventListener("focusin", focused);
		xterm.focus();
		// Settings → Appearance: its colours and text size follow at once.
		const restyle = onAppearance(() => {
			xterm.options.theme = theme();
			xterm.options.fontFamily = codeFont();
			xterm.options.fontSize = Math.round(13 * textScale());
			resize();
		});

		return () => {
			at.stopped = true;
			clearTimeout(at.retry);
			observer.disconnect();
			element.removeEventListener("focusin", focused);
			restyle();
			typing.dispose();
			links.dispose();
			selecting.dispose();
			binary.dispose();
			at.socket?.close();
			xterm.dispose();
		};
	}, [id]);

	const find = (previous: boolean) => {
		const search = handle.current.search;
		if (!search || !query) return;
		if (previous) search.findPrevious(query);
		else search.findNext(query);
	};
	const closeSearch = () => {
		setSearching(false);
		handle.current.search?.clearDecorations();
		handle.current.xterm?.focus();
	};
	const key = (data: string) => {
		handle.current.send(data);
		handle.current.xterm?.focus();
	};

	return (
		<div className="bg-background relative flex h-full min-h-0 flex-col">
			<div ref={host} className="min-h-0 flex-1 px-2 pt-1.5" />
			{chat && selected.trim() ? (
				<div className="absolute right-3 bottom-3 z-10">
					<AddToChat
						references={() => [
							{
								id: referenceId(),
								kind: "terminalOutput",
								// The agent reads it, in the context the server writes (English).
								label: terminal.title ?? terminal.command ?? "the shell",
								text: selected.replace(/\s+$/, ""),
							},
						]}
					/>
				</div>
			) : null}
			{searching ? (
				<div className="bg-popover border-border absolute top-2 right-3 z-10 flex items-center gap-1 rounded-md border p-1 shadow-md">
					<input
						// oxlint-disable-next-line jsx-a11y/no-autofocus
						autoFocus
						aria-label={t("terminal.find")}
						placeholder={t("terminal.findPlaceholder")}
						value={query}
						onChange={(event) => {
							setQuery(event.target.value);
							if (event.target.value) handle.current.search?.findNext(event.target.value);
						}}
						onKeyDown={(event) => {
							if (event.key === "Enter") find(event.shiftKey);
							else if (event.key === "Escape") closeSearch();
						}}
						className="w-44 bg-transparent px-1.5 py-0.5 text-xs outline-none"
					/>
					<IconButton label={t("terminal.previous")} onClick={() => find(true)}>
						<ChevronUp />
					</IconButton>
					<IconButton label={t("terminal.next")} onClick={() => find(false)}>
						<ChevronDown />
					</IconButton>
					<IconButton label={t("common:actions.close")} onClick={closeSearch}>
						<X />
					</IconButton>
				</div>
			) : null}
			{touch ? (
				<div
					role="toolbar"
					aria-label={t("terminal.keys")}
					className="border-border flex shrink-0 gap-1 overflow-x-auto border-t px-2 py-1.5"
				>
					{(
						[
							["Esc", "\u001b"],
							["Tab", "\t"],
						] as const
					).map(([label, data]) => (
						<Key key={label} label={label} onPress={() => key(data)} />
					))}
					<Key
						label="Ctrl"
						pressed={ctrl}
						onPress={() => {
							ctrlArmed.current = !ctrlArmed.current;
							setCtrl(ctrlArmed.current);
							handle.current.xterm?.focus();
						}}
					/>
					{(
						[
							["←", "\u001b[D"],
							["↑", "\u001b[A"],
							["↓", "\u001b[B"],
							["→", "\u001b[C"],
							["|", "|"],
							["~", "~"],
							["/", "/"],
							["-", "-"],
						] as const
					).map(([label, data]) => (
						<Key key={label} label={label} onPress={() => key(data)} />
					))}
				</div>
			) : null}
			{link === "reconnecting" ? (
				<p className="bg-popover text-muted-foreground border-border absolute top-2 right-3 rounded-md border px-2 py-1 text-xs">
					{t("terminal.reconnecting")}
				</p>
			) : null}
			{exited !== undefined ? (
				<div className="border-border bg-card flex shrink-0 items-center gap-3 border-t px-3 py-2 text-xs">
					<span className="text-muted-foreground">
						{exited === null
							? t("terminal.machineRestarted")
							: t("terminal.endedWith", { code: exited })}
					</span>
					<Button
						size="xs"
						variant="outline"
						disabled={restart.isPending}
						onClick={() =>
							restart.mutate({ id }, { onSuccess: () => setExited(undefined), onError: toastError })
						}
					>
						<RotateCcw />
						{t("terminal.restart")}
					</Button>
				</div>
			) : null}
		</div>
	);
}

/** A key of the phone's key row. */
function Key({
	label,
	pressed,
	onPress,
}: {
	label: string;
	pressed?: boolean;
	onPress: () => void;
}) {
	return (
		<button
			type="button"
			aria-pressed={pressed}
			// Keeps the keyboard up: the terminal keeps the focus.
			onPointerDown={(event) => event.preventDefault()}
			onClick={onPress}
			className={
				pressed
					? "bg-primary text-primary-foreground h-8 min-w-9 shrink-0 rounded-md px-2 font-mono text-xs"
					: "bg-muted text-foreground h-8 min-w-9 shrink-0 rounded-md px-2 font-mono text-xs"
			}
		>
			{label}
		</button>
	);
}
