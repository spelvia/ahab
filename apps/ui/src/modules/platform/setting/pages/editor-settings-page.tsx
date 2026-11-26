import { SettingsFields } from "@/modules/platform/setting/components/settings-fields";
import { CHAT_FIELDS } from "@/modules/platform/setting/model";
import { useTranslation } from "@/shared/i18n";

/**
 * Settings → Editor: how file windows look and behave, for everyone on this
 * server. The chat composer's fields are on Settings → Agent editor.
 */
export function EditorSettingsPage() {
	const { t } = useTranslation("platform");
	return (
		<section className="space-y-8">
			<header className="space-y-1">
				<h1 className="text-2xl font-semibold">{t("editor.title")}</h1>
				<p className="text-muted-foreground max-w-2xl text-sm text-pretty">{t("editor.intro")}</p>
			</header>
			<SettingsFields section="editor" except={CHAT_FIELDS} />
		</section>
	);
}
