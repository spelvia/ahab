import { cn } from "cn";
import { useEffect, useMemo, useRef, useState } from "react";

import type { DocumentSymbol } from "@/shared/editor/model";
import { useTranslation } from "@/shared/i18n";

/**
 * A file's outline: its definitions, nested, from the syntax
 * tree (or from the language server, when one runs). Clicking one
 * selects its name in the editor.
 */
export function Outline({
	symbols,
	onReveal,
	focusFilter,
}: {
	/** Null: the language has no outline. Undefined: still finding it. */
	symbols: DocumentSymbol[] | null | undefined;
	onReveal: (symbol: DocumentSymbol) => void;
	/** Changes when the filter should take the focus (⇧⌘O). */
	focusFilter?: number;
}) {
	const { t } = useTranslation("projects");
	const [filter, setFilter] = useState("");
	const input = useRef<HTMLInputElement>(null);
	useEffect(() => {
		if (focusFilter) input.current?.focus();
	}, [focusFilter]);
	const shown = useMemo(() => {
		if (!symbols || !filter.trim()) return symbols;
		const wanted = filter.trim().toLowerCase();
		const flat: DocumentSymbol[] = [];
		const walk = (list: DocumentSymbol[]) => {
			for (const symbol of list) {
				if (symbol.name.toLowerCase().includes(wanted)) flat.push({ ...symbol, children: [] });
				walk(symbol.children);
			}
		};
		walk(symbols);
		return flat;
	}, [symbols, filter]);
	return (
		<aside
			aria-label={t("outline.title")}
			className="border-border flex min-h-0 w-60 shrink-0 flex-col border-l text-xs max-md:hidden"
		>
			<div className="border-border flex h-8 shrink-0 items-center gap-2 border-b px-2">
				<span className="text-muted-foreground font-medium">{t("outline.title")}</span>
				<input
					ref={input}
					value={filter}
					onChange={(event) => setFilter(event.target.value)}
					onKeyDown={(event) => {
						if (event.key === "Enter" && shown?.[0]) onReveal(shown[0]);
					}}
					placeholder={t("outline.filter")}
					aria-label={t("outline.filterLabel")}
					className="bg-background border-input min-w-0 flex-1 rounded border px-1.5 py-0.5 outline-none"
				/>
			</div>
			<div className="min-h-0 flex-1 overflow-auto py-1">
				{shown === undefined ? (
					<p className="text-muted-foreground px-3 py-2">{t("outline.finding")}</p>
				) : shown === null ? (
					<p className="text-muted-foreground px-3 py-2">{t("outline.none")}</p>
				) : shown.length === 0 ? (
					<p className="text-muted-foreground px-3 py-2">
						{filter ? t("outline.noMatch") : t("outline.empty")}
					</p>
				) : (
					<SymbolList symbols={shown} depth={0} onReveal={onReveal} />
				)}
			</div>
		</aside>
	);
}

function SymbolList({
	symbols,
	depth,
	onReveal,
}: {
	symbols: DocumentSymbol[];
	depth: number;
	onReveal: (symbol: DocumentSymbol) => void;
}) {
	return (
		<ul>
			{symbols.map((symbol) => (
				<li key={`${symbol.from}:${symbol.nameFrom}`}>
					<button
						type="button"
						onClick={() => onReveal(symbol)}
						title={`${symbol.kind} ${symbol.name}`}
						className={cn(
							"hover:bg-accent/60 flex w-full items-baseline gap-1.5 truncate py-0.5 pr-2 text-left",
						)}
						style={{ paddingLeft: `${0.75 + depth * 0.75}rem` }}
					>
						<span className="text-muted-foreground w-8 shrink-0 text-[0.625rem] uppercase">
							{abbreviation(symbol.kind)}
						</span>
						<span className="text-foreground truncate font-mono">{symbol.name}</span>
					</button>
					{symbol.children.length ? (
						<SymbolList symbols={symbol.children} depth={depth + 1} onReveal={onReveal} />
					) : null}
				</li>
			))}
		</ul>
	);
}

/** A short label for a kind: `function` → `fn`. */
function abbreviation(kind: string): string {
	const known: Record<string, string> = {
		function: "fn",
		method: "fn",
		class: "class",
		struct: "struct",
		interface: "iface",
		enum: "enum",
		type: "type",
		module: "mod",
		constant: "const",
		macro: "macro",
		section: "§",
		field: "field",
		variable: "var",
	};
	return known[kind.split(".")[0] ?? kind] ?? kind.slice(0, 5);
}
