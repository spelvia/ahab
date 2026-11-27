import {
	FolderTree,
	GitBranch,
	type LucideIcon,
	MessagesSquare,
	Search,
	SquareTerminal,
} from "lucide-react";

import { useFeatures, useSetFeatureEnabled } from "@/modules/platform/feature/api";
import type { CoreFeature } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Switch } from "@/shared/ui/switch";
import { toastError } from "@/shared/ui/toast";

const ICONS: Record<string, LucideIcon> = {
	sessions: MessagesSquare,
	terminal: SquareTerminal,
	files: FolderTree,
	git: GitBranch,
	search: Search,
};

/** Settings → Features: Ahab's own parts, each switched on or off for everyone. */
export function FeaturesPage() {
	const { t } = useTranslation("platform");
	const features = useFeatures();

	return (
		<section className="space-y-8">
			<header className="space-y-1">
				<h1 className="text-2xl font-semibold">{t("features.title")}</h1>
				<p className="text-muted-foreground max-w-2xl text-sm text-pretty">{t("features.intro")}</p>
			</header>
			<ul className="border-border divide-border divide-y border-y">
				{(features.data?.items ?? []).map((feature) => (
					<FeatureRow key={feature.id} feature={feature} />
				))}
			</ul>
		</section>
	);
}

function FeatureRow({ feature }: { feature: CoreFeature }) {
	const { t } = useTranslation("platform");
	const change = useSetFeatureEnabled();
	const Icon = ICONS[feature.id] ?? MessagesSquare;
	const pending = change.isPending ? change.variables?.data.enabled : undefined;
	const on = pending ?? feature.enabled;

	return (
		<li className="flex items-center gap-4 px-3 py-4">
			<span
				className={`flex size-9 shrink-0 items-center justify-center rounded-lg border transition-colors duration-150 ${
					on
						? "border-primary/30 bg-primary/10 text-primary"
						: "border-border bg-muted text-muted-foreground"
				}`}
			>
				<Icon className="size-4.5" aria-hidden />
			</span>
			<div className="min-w-0 flex-1">
				<p className={`font-semibold ${on ? "text-heading" : "text-muted-foreground"}`}>
					{feature.name}
				</p>
				<p className="text-muted-foreground text-sm text-pretty">{feature.description}</p>
			</div>
			{feature.required ? (
				<span className="text-muted-foreground text-xs whitespace-nowrap">
					{t("features.alwaysOn")}
				</span>
			) : (
				<Switch
					checked={on}
					disabled={change.isPending}
					label={t(on ? "features.switchOn" : "features.switchOff", { feature: feature.name })}
					onCheckedChange={(enabled) =>
						change.mutate({ id: feature.id, data: { enabled } }, { onError: toastError })
					}
				/>
			)}
		</li>
	);
}
