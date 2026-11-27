import { Fragment, useEffect, useState } from "react";

import {
	useNotificationSettings,
	useSaveNotificationSettings,
} from "@/modules/notifications/notification/api";
import {
	changeNotificationSound,
	useNotificationSound,
} from "@/modules/notifications/notification/hooks";
import { bySource } from "@/modules/notifications/notification/model";
import { PhoneRows, usePhoneApp } from "@/modules/notifications/phone";
import { PushRows } from "@/modules/notifications/push";
import type { NotificationSettings } from "@/shared/api/generated/ahab.schemas";
import { app } from "@/shared/desktop/bridge";
import { useTranslation } from "@/shared/i18n";
import { playTone, type Tone, TONES } from "@/shared/sound";
import { Switch } from "@/shared/ui/switch";

const select =
	"border-input bg-background focus-visible:ring-ring/50 h-8 rounded-md border px-2 text-sm outline-none focus-visible:ring-[3px]";

/** What this browser allows, for notifications outside the page. */
function useBrowserPermission() {
	const supported = typeof window !== "undefined" && "Notification" in window;
	const [permission, setPermission] = useState<NotificationPermission>(
		supported ? window.Notification.permission : "denied",
	);
	const ask = async () => {
		if (supported) setPermission(await window.Notification.requestPermission());
	};
	return { supported, permission, ask };
}

/** Settings → Notifications: on or off, which kinds, outside the app, how long they are kept. */
export function NotificationsPage() {
	const { t } = useTranslation(["notifications", "common"]);
	const settings = useNotificationSettings();
	const save = useSaveNotificationSettings();
	const browser = useBrowserPermission();
	const [inApp, setInApp] = useState(false);
	const phone = usePhoneApp();
	useEffect(() => {
		void app.info().then((info) => setInApp(Boolean(info)));
	}, []);

	if (settings.isPending)
		return <p className="text-muted-foreground text-sm">{t("common:state.loading")}</p>;
	if (!settings.data) return <p className="text-muted-foreground text-sm">{t("page.loadError")}</p>;
	const current = settings.data;
	const change = (next: Partial<NotificationSettings>) => {
		const merged = { ...current, ...next };
		save.mutate({
			data: {
				enabled: merged.enabled,
				keepDays: merged.keepDays ?? null,
				outside: merged.outside,
				kinds: Object.fromEntries(merged.kinds.map((kind) => [kind.id, kind.enabled])),
				insideOnly: merged.kinds.filter((kind) => !kind.outside).map((kind) => kind.id),
			},
		});
	};
	const keep = current.keepDays ?? null;

	return (
		<section className="space-y-8">
			<header className="space-y-1">
				<h1 className="text-2xl font-semibold">{t("page.title")}</h1>
				<p className="text-muted-foreground max-w-2xl text-sm text-pretty">
					{t("page.description")}
				</p>
			</header>

			<ul className="border-border divide-border divide-y border-y">
				<Row title={t("page.enabled.title")} description={t("page.enabled.description")}>
					<Switch
						label={t("page.enabled.title")}
						checked={current.enabled}
						onCheckedChange={(enabled) => change({ enabled })}
					/>
				</Row>
				{bySource(current.kinds).map((group) => (
					<Fragment key={group.source}>
						{group.source === "ahab" ? null : (
							<li className="text-muted-foreground px-3 pt-4 pb-1 text-xs font-semibold tracking-wide uppercase">
								{group.name}
							</li>
						)}
						{group.kinds.map((kind) => (
							<Row
								key={kind.id}
								title={kind.title}
								description={kind.description}
								muted={!current.enabled}
							>
								<select
									aria-label={kind.title}
									value={!kind.enabled ? "off" : kind.outside ? "everywhere" : "inside"}
									disabled={!current.enabled}
									onChange={(event) => {
										const where = event.target.value;
										change({
											kinds: current.kinds.map((other) =>
												other.id === kind.id
													? { ...other, enabled: where !== "off", outside: where === "everywhere" }
													: other,
											),
										});
									}}
									className={select}
								>
									<option value="everywhere">{t("page.kind.everywhere")}</option>
									<option value="inside">{t("page.kind.inside")}</option>
									<option value="off">{t("page.kind.off")}</option>
								</select>
							</Row>
						))}
					</Fragment>
				))}
			</ul>

			<SoundSection muted={!current.enabled} />

			<section className="space-y-3">
				<h2 className="text-heading text-base font-bold">{t("page.outside.heading")}</h2>
				<ul className="border-border divide-border divide-y border-y">
					<Row
						title={t("page.outside.title")}
						description={
							phone
								? t("page.outside.phone")
								: inApp
									? t("page.outside.desktop")
									: t("page.outside.browser")
						}
						muted={!current.enabled}
					>
						<Switch
							label={t("page.outside.title")}
							checked={current.outside}
							disabled={!current.enabled}
							onCheckedChange={(outside) => change({ outside })}
						/>
					</Row>
					{phone ? <PhoneRows muted={!current.enabled || !current.outside} /> : null}
					{inApp ? null : (
						<PushRows
							muted={!current.enabled || !current.outside}
							permission={{
								supported: browser.supported,
								state: browser.permission,
								ask: browser.ask,
							}}
						/>
					)}
				</ul>
			</section>

			<section className="space-y-3">
				<h2 className="text-heading text-base font-bold">{t("page.keeping.heading")}</h2>
				<ul className="border-border divide-border divide-y border-y">
					<Row title={t("page.keeping.title")} description={t("page.keeping.description")}>
						<select
							aria-label={t("page.keeping.label")}
							className={`${select} w-40`}
							value={keep === null ? "forever" : String(keep)}
							onChange={(event) =>
								change({
									keepDays: event.target.value === "forever" ? null : Number(event.target.value),
								})
							}
						>
							{[7, 30, 90, 365].map((days) => (
								<option key={days} value={days}>
									{t("page.keeping.days", { count: days })}
								</option>
							))}
							{keep !== null && ![7, 30, 90, 365].includes(keep) ? (
								<option value={keep}>{t("page.keeping.days", { count: keep })}</option>
							) : null}
							<option value="forever">{t("page.keeping.forever")}</option>
						</select>
					</Row>
				</ul>
			</section>
			{save.isError ? (
				<p role="alert" className="text-destructive text-sm">
					{t("page.saveError")}
				</p>
			) : null}
		</section>
	);
}

/** This device's sound as notifications arrive: on or off, which, how loud. */
function SoundSection({ muted }: { muted: boolean }) {
	const { t } = useTranslation("notifications");
	const sound = useNotificationSound();
	return (
		<section className="space-y-3">
			<h2 className="text-heading text-base font-bold">{t("page.sound.heading")}</h2>
			<ul className="border-border divide-border divide-y border-y">
				<Row title={t("page.sound.title")} description={t("page.sound.description")} muted={muted}>
					<Switch
						label={t("page.sound.title")}
						checked={sound.on}
						onCheckedChange={(on) => {
							changeNotificationSound({ on });
							if (on) playTone(sound.tone, sound.volume);
						}}
					/>
				</Row>
				<Row
					title={t("page.sound.tone")}
					description={t("page.sound.toneDescription")}
					muted={muted || !sound.on}
				>
					<div className="flex items-center gap-2">
						<input
							type="range"
							min={0.1}
							max={1}
							step={0.1}
							value={sound.volume}
							aria-label={t("page.sound.volume")}
							disabled={!sound.on}
							onChange={(event) =>
								changeNotificationSound({ volume: event.currentTarget.valueAsNumber })
							}
							onPointerUp={() => playTone(sound.tone, sound.volume)}
							className="accent-primary w-24"
						/>
						<select
							aria-label={t("page.sound.tone")}
							value={sound.tone}
							disabled={!sound.on}
							onChange={(event) => {
								const tone = event.target.value as Tone;
								changeNotificationSound({ tone });
								playTone(tone, sound.volume);
							}}
							className={select}
						>
							{TONES.map((tone) => (
								<option key={tone} value={tone}>
									{t(`page.sound.tones.${tone}`)}
								</option>
							))}
						</select>
						<button
							type="button"
							disabled={!sound.on}
							onClick={() => playTone(sound.tone, sound.volume)}
							className="text-primary hover:bg-accent rounded-md px-2 py-1 text-sm font-medium disabled:opacity-50"
						>
							{t("page.sound.play")}
						</button>
					</div>
				</Row>
			</ul>
		</section>
	);
}

function Row({
	title,
	description,
	muted,
	children,
}: {
	title: string;
	description: string;
	muted?: boolean;
	children: React.ReactNode;
}) {
	return (
		<li className={`flex items-center gap-4 px-3 py-3.5 ${muted ? "opacity-60" : ""}`}>
			<div className="min-w-0 flex-1">
				<p className="text-heading text-sm font-semibold">{title}</p>
				<p className="text-muted-foreground text-sm text-pretty">{description}</p>
			</div>
			{children}
		</li>
	);
}
