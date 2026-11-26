import type { ReactNode } from "react";

import { SettingsFields } from "@/modules/platform/setting/components/settings-fields";
import { CHAT_FIELDS } from "@/modules/platform/setting/model";
import { type Appearance, changeAppearance, useAppearance } from "@/shared/appearance";
import { useTranslation } from "@/shared/i18n";
import { Switch } from "@/shared/ui/switch";

type Toggle = keyof Pick<Appearance, "expandThinking" | "expandTools" | "showTokens" | "showCost">;

/**
 * Settings → Agent editor: how chats with agents read and how you write to
 * them. The transcript and what Session usage shows are this device's
 * (phones and desktops differ); the composer's fields are the server's.
 */
export function AgentEditorPage() {
	const { t } = useTranslation("platform");
	const shown = useAppearance();
	const row = (key: Toggle, label: string, description: string) => (
		<li key={key} className="flex items-center gap-4 px-3 py-3.5">
			<div className="min-w-0 flex-1">
				<p className="text-heading text-sm font-semibold">{label}</p>
				<p className="text-muted-foreground text-sm text-pretty">{description}</p>
			</div>
			<Switch
				label={label}
				checked={shown[key]}
				onCheckedChange={(checked) => changeAppearance({ [key]: checked })}
			/>
		</li>
	);
	return (
		<section className="max-w-3xl space-y-10">
			<header className="space-y-1">
				<h1 className="text-2xl font-semibold">{t("agentEditor.title")}</h1>
				<p className="text-muted-foreground text-sm text-pretty">{t("agentEditor.intro")}</p>
			</header>
			<Group
				title={t("agentEditor.transcript.title")}
				description={t("agentEditor.transcript.description")}
			>
				<ul className="border-border divide-border divide-y border-y">
					{row(
						"expandThinking",
						t("agentEditor.transcript.thinking"),
						t("agentEditor.transcript.thinkingDescription"),
					)}
					{row(
						"expandTools",
						t("agentEditor.transcript.tools"),
						t("agentEditor.transcript.toolsDescription"),
					)}
				</ul>
			</Group>
			<Group title={t("agentEditor.usage.title")} description={t("agentEditor.usage.description")}>
				<ul className="border-border divide-border divide-y border-y">
					{row(
						"showTokens",
						t("agentEditor.usage.tokens"),
						t("agentEditor.usage.tokensDescription"),
					)}
					{row("showCost", t("agentEditor.usage.cost"), t("agentEditor.usage.costDescription"))}
				</ul>
			</Group>
			<Group
				title={t("agentEditor.composer.title")}
				description={t("agentEditor.composer.description")}
			>
				<SettingsFields section="editor" only={CHAT_FIELDS} />
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
	description: string;
	children: ReactNode;
}) {
	return (
		<section className="space-y-3">
			<div className="space-y-0.5">
				<h2 className="text-heading text-base font-bold">{title}</h2>
				<p className="text-muted-foreground text-sm">{description}</p>
			</div>
			{children}
		</section>
	);
}
