import { cn } from "cn";
import { Check, Monitor, Moon, Sun } from "lucide-react";
import type { ReactNode } from "react";

import { FontPicker, ThemePicker } from "@/modules/platform/setting/components/theme-picker";
import {
	clampTextPercent,
	changeAppearance,
	type Theme,
	TEXT_PERCENT,
	useAppearance,
} from "@/shared/appearance";
import { formatNumber, LANGUAGES, useTranslation } from "@/shared/i18n";
import { Switch } from "@/shared/ui/switch";

const THEMES: { id: Theme; icon: typeof Sun }[] = [
	{ id: "system", icon: Monitor },
	{ id: "light", icon: Sun },
	{ id: "dark", icon: Moon },
];

/**
 * Settings → Appearance: how Ahab looks on this device (browser or app):
 * its theme, its text size (the editor and terminals included), motion and
 * language. Kept here, not on the server: a phone and a desktop differ.
 */
export function AppearancePage() {
	const { t } = useTranslation("platform");
	const shown = useAppearance();
	return (
		<section className="max-w-3xl space-y-10">
			<header className="space-y-1">
				<h1 className="text-2xl font-semibold">{t("appearance.title")}</h1>
				<p className="text-muted-foreground text-sm text-pretty">{t("appearance.intro")}</p>
			</header>

			<Group title={t("appearance.theme.title")}>
				<div
					role="radiogroup"
					aria-label={t("appearance.theme.title")}
					className="grid gap-3 sm:grid-cols-3"
				>
					{THEMES.map(({ id, icon: Icon }) => {
						const chosen = shown.theme === id;
						return (
							<button
								key={id}
								type="button"
								role="radio"
								aria-checked={chosen}
								onClick={() => changeAppearance({ theme: id })}
								className={cn(
									"border-border hover:border-muted-foreground/60 focus-visible:ring-ring group flex flex-col overflow-hidden rounded-lg border text-left outline-none focus-visible:ring-2",
									chosen && "border-primary ring-primary/40 ring-1",
								)}
							>
								<ThemePreview theme={id} />
								<span className="flex items-center gap-2 px-3 py-2.5 text-sm">
									<Icon className="text-muted-foreground size-4" />
									<span className="text-heading flex-1 font-medium">
										{t(`appearance.theme.${id}`)}
									</span>
									{chosen ? <Check className="text-primary size-4" /> : null}
								</span>
							</button>
						);
					})}
				</div>
				{shown.theme === "system" ? (
					<p className="text-muted-foreground text-xs">
						{t(shown.dark ? "appearance.theme.systemNowDark" : "appearance.theme.systemNowLight")}
					</p>
				) : null}
			</Group>

			<Group
				title={t("appearance.colorTheme.title")}
				description={t("appearance.colorTheme.description")}
			>
				<ThemePicker />
			</Group>

			<Group title={t("appearance.fonts.title")} description={t("appearance.fonts.description")}>
				<FontPicker />
			</Group>

			<Group
				title={t("appearance.textSize.title")}
				description={t("appearance.textSize.description")}
			>
				<div className="flex max-w-md flex-wrap items-center gap-3">
					<input
						type="range"
						min={TEXT_PERCENT.min}
						max={TEXT_PERCENT.max}
						step={TEXT_PERCENT.step}
						value={shown.textPercent}
						aria-label={t("appearance.textSize.title")}
						aria-valuetext={formatNumber(shown.textPercent / 100, {
							style: "percent",
							maximumFractionDigits: 1,
						})}
						onChange={(event) =>
							changeAppearance({ textPercent: event.currentTarget.valueAsNumber })
						}
						className="accent-primary min-w-20 flex-1 cursor-pointer"
					/>
					<label className="border-input focus-within:ring-ring flex items-center gap-1 rounded-md border px-2 py-1 text-sm tabular-nums focus-within:ring-2">
						<input
							key={shown.textPercent}
							type="number"
							aria-label={t("appearance.textSize.percentage")}
							min={TEXT_PERCENT.min}
							max={TEXT_PERCENT.max}
							step={TEXT_PERCENT.step}
							defaultValue={shown.textPercent}
							onBlur={(event) => {
								const input = event.currentTarget;
								const value = input.valueAsNumber;
								if (!Number.isFinite(value)) {
									input.value = String(shown.textPercent);
									return;
								}
								const percent = clampTextPercent(value);
								input.value = String(percent);
								changeAppearance({ textPercent: percent });
							}}
							onKeyDown={(event) => {
								if (event.key === "Enter") event.currentTarget.blur();
							}}
							className="w-14 bg-transparent outline-none"
						/>
						<span aria-hidden className="text-muted-foreground">
							%
						</span>
					</label>
					<button
						type="button"
						disabled={shown.textPercent === TEXT_PERCENT.default}
						onClick={() => changeAppearance({ textPercent: TEXT_PERCENT.default })}
						className="text-muted-foreground hover:text-foreground rounded px-1 text-sm disabled:opacity-50"
					>
						{t("appearance.textSize.reset")}
					</button>
				</div>
			</Group>

			<Group title={t("appearance.motion.title")}>
				<ul className="border-border divide-border divide-y rounded-lg border">
					<li className="flex items-center gap-4 px-4 py-3">
						<div className="min-w-0 flex-1">
							<p className="text-heading text-sm font-medium">{t("appearance.motion.reduce")}</p>
							<p className="text-muted-foreground text-sm">
								{t("appearance.motion.reduceDescription")}
							</p>
						</div>
						<Switch
							label={t("appearance.motion.reduce")}
							checked={shown.motion === "reduce"}
							onCheckedChange={(reduce) =>
								changeAppearance({ motion: reduce ? "reduce" : "system" })
							}
						/>
					</li>
				</ul>
			</Group>

			<Group
				title={t("appearance.language.title")}
				description={t("appearance.language.description")}
			>
				<select
					aria-label={t("appearance.language.title")}
					value={shown.language}
					onChange={(event) => changeAppearance({ language: event.target.value })}
					disabled={LANGUAGES.length < 2}
					className="border-input bg-background focus-visible:ring-ring h-9 rounded-md border px-2.5 text-sm outline-none focus-visible:ring-2 disabled:opacity-70"
				>
					{LANGUAGES.map((language) => (
						<option key={language.code} value={language.code}>
							{language.name}
						</option>
					))}
				</select>
			</Group>
		</section>
	);
}

function Group({
	title,
	description,
	children,
}: {
	title: string;
	description?: string;
	children: ReactNode;
}) {
	return (
		<section className="space-y-3">
			<div className="space-y-0.5">
				<h2 className="text-heading text-base font-bold">{title}</h2>
				{description ? <p className="text-muted-foreground text-sm">{description}</p> : null}
			</div>
			{children}
		</section>
	);
}

/** A tiny window in a theme's colours: dark, light, or half of each. */
function ThemePreview({ theme }: { theme: Theme }) {
	const dark = { bg: "#252526", bar: "#2d2d30", line: "#4a4d50", accent: "#8fb3a3" };
	const light = { bg: "#f4f2ea", bar: "#ebe8de", line: "#cfcdc2", accent: "#315f57" };
	const pane = (colours: typeof dark, clip?: string) => (
		<div className="absolute inset-0 p-3" style={{ background: colours.bg, clipPath: clip }}>
			<div className="mb-2 h-2 w-full rounded-sm" style={{ background: colours.bar }} />
			<div className="flex gap-2">
				<div className="h-12 w-8 rounded-sm" style={{ background: colours.bar }} />
				<div className="flex-1 space-y-1.5 pt-1">
					<div className="h-1.5 w-3/4 rounded-full" style={{ background: colours.accent }} />
					<div className="h-1.5 w-full rounded-full" style={{ background: colours.line }} />
					<div className="h-1.5 w-2/3 rounded-full" style={{ background: colours.line }} />
				</div>
			</div>
		</div>
	);
	return (
		<div aria-hidden className="border-border relative h-24 border-b">
			{theme === "light" ? pane(light) : pane(dark)}
			{theme === "system" ? pane(light, "polygon(55% 0, 100% 0, 100% 100%, 45% 100%)") : null}
		</div>
	);
}
