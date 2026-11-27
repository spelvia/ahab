import { cn } from "cn";
import { Check } from "lucide-react";
import { useState } from "react";

import { changeAppearance, useAppearance } from "@/shared/appearance";
import { useTranslation } from "@/shared/i18n";
import { COLOR_THEMES, type ColorTheme, variantOf } from "@/shared/themes";

/** Ahab's own colours, for its swatch: the stylesheet's, light and dark. */
const AHAB = {
	light: ["#f4f2ea", "#315f57", "#a4412f", "#89590f", "#44701b", "#2f5d9e"],
	dark: ["#252526", "#8fb3a3", "#e39a8a", "#d9b26a", "#6a9955", "#569cd6"],
};

/** A theme's page and accents, as the swatch shows them. */
function swatch(theme: ColorTheme | undefined, dark: boolean): string[] {
	if (!theme) return dark ? AHAB.dark : AHAB.light;
	const { colors } = variantOf(theme, dark);
	return [colors.background, colors.primary, colors.red, colors.yellow, colors.green, colors.blue];
}

/**
 * Settings → Appearance → Colour theme: Ahab's own, or a well-known one,
 * shown in the light or dark chosen above (a theme with only one shows it).
 */
export function ThemePicker() {
	const { t } = useTranslation("platform");
	const shown = useAppearance();
	const themes: (ColorTheme | undefined)[] = [undefined, ...COLOR_THEMES];
	return (
		<div
			role="radiogroup"
			aria-label={t("appearance.colorTheme.title")}
			className="grid grid-cols-2 gap-2 sm:grid-cols-3"
		>
			{themes.map((theme) => {
				const id = theme?.id ?? "";
				const chosen = shown.colorTheme === id;
				const [page, ...accents] = swatch(theme, shown.dark);
				const only = theme && (!theme.light || !theme.dark);
				return (
					<button
						key={id || "ahab"}
						type="button"
						role="radio"
						aria-checked={chosen}
						onClick={() => changeAppearance({ colorTheme: id })}
						className={cn(
							"border-border hover:border-muted-foreground/60 focus-visible:ring-ring flex items-center gap-2.5 rounded-lg border p-2 text-left outline-none focus-visible:ring-2",
							chosen && "border-primary ring-primary/40 ring-1",
						)}
					>
						<span
							aria-hidden
							className="border-border flex h-8 w-12 shrink-0 items-end gap-0.5 rounded-md border p-1"
							style={{ background: page }}
						>
							{accents.map((color) => (
								<span
									key={color}
									className="h-2.5 flex-1 rounded-sm"
									style={{ background: color }}
								/>
							))}
						</span>
						<span className="min-w-0 flex-1">
							<span className="text-heading block truncate text-sm font-medium">
								{theme?.name ?? t("appearance.colorTheme.ahab")}
							</span>
							<span className="text-muted-foreground block truncate text-xs">
								{only
									? theme.dark
										? t("appearance.colorTheme.darkOnly")
										: t("appearance.colorTheme.lightOnly")
									: t("appearance.colorTheme.both")}
							</span>
						</span>
						{chosen ? <Check className="text-primary size-4 shrink-0" /> : null}
					</button>
				);
			})}
		</div>
	);
}

const field =
	"border-input bg-background focus-visible:ring-ring h-9 rounded-md border px-2.5 text-sm outline-none focus-visible:ring-2";

/**
 * Settings → Appearance → Fonts: the interface's and code's (the editor and
 * terminals). Ahab's, the system's, or any font installed here, by name.
 */
export function FontPicker() {
	const { t } = useTranslation("platform");
	const shown = useAppearance();
	return (
		<ul className="border-border divide-border divide-y rounded-lg border">
			<FontRow
				key={`ui:${shown.uiFont}`}
				label={t("appearance.fonts.ui")}
				ahab="Inter"
				value={shown.uiFont}
				onChange={(uiFont) => changeAppearance({ uiFont })}
			/>
			<FontRow
				key={`code:${shown.codeFont}`}
				label={t("appearance.fonts.code")}
				ahab="JetBrains Mono"
				value={shown.codeFont}
				mono
				onChange={(codeFont) => changeAppearance({ codeFont })}
			/>
		</ul>
	);
}

function FontRow({
	label,
	ahab,
	value,
	mono,
	onChange,
}: {
	label: string;
	/** Ahab's own font's name. */
	ahab: string;
	value: string;
	mono?: boolean;
	onChange: (value: string) => void;
}) {
	const { t } = useTranslation("platform");
	const kind = value === "" || value === "system" ? value : "other";
	const [typing, setTyping] = useState(kind === "other" ? value : "");
	const [other, setOther] = useState(kind === "other");
	return (
		<li className="flex flex-wrap items-center gap-3 px-4 py-3">
			<span className="text-heading min-w-32 flex-1 text-sm font-medium">{label}</span>
			<select
				aria-label={label}
				value={other ? "other" : kind}
				onChange={(event) => {
					const chosen = event.target.value;
					setOther(chosen === "other");
					if (chosen !== "other") onChange(chosen);
					else if (typing.trim()) onChange(typing.trim());
				}}
				className={field}
			>
				<option value="">{t("appearance.fonts.ahab", { font: ahab })}</option>
				<option value="system">{t("appearance.fonts.system")}</option>
				<option value="other">{t("appearance.fonts.other")}</option>
			</select>
			{other ? (
				<input
					value={typing}
					aria-label={t("appearance.fonts.name", { what: label })}
					placeholder={mono ? "Fira Code" : "IBM Plex Sans"}
					onChange={(event) => setTyping(event.target.value)}
					onBlur={() => typing.trim() && onChange(typing.trim())}
					onKeyDown={(event) => {
						if (event.key === "Enter") event.currentTarget.blur();
					}}
					className={cn(field, "w-44", mono && "font-mono")}
				/>
			) : null}
		</li>
	);
}
