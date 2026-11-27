import { cn } from "cn";
import { RotateCw, Search } from "lucide-react";
import { useState } from "react";

import { useModels, useRefreshModels } from "@/modules/sessions/chat/api";
import type { AgentModel } from "@/shared/api/generated/ahab.schemas";
import { agoInWords } from "@/shared/format";
import { formatDate, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/** How many models show before "Show all". */
const FIRST = 6;
/** From how many models a filter helps. */
const FILTER_FROM = 8;

/**
 * The models an agent offers on a machine, as Ahab keeps them: the default
 * first, each with its ID and reasoning efforts (the default one marked);
 * filterable when there are many; when they were read, and read again on
 * request.
 */
export function AgentModels({ machineId, agent }: { machineId: string; agent: string }) {
	const { t } = useTranslation("sessions");
	const models = useModels(machineId, agent);
	const refresh = useRefreshModels(machineId, agent);
	const [all, setAll] = useState(false);
	const [query, setQuery] = useState("");

	if (models.isPending)
		return <p className="text-muted-foreground text-xs">{t("agentModels.asking", { agent })}</p>;
	if (models.isError)
		return (
			<div className="space-y-2">
				<p className="text-muted-foreground text-xs">
					{t("agentModels.cannotRead", { error: models.error.message })}
				</p>
				<ReadAgain pending={refresh.isPending} onClick={() => refresh.mutate()} />
			</div>
		);

	const wanted = query.trim().toLowerCase();
	const items = models.data.items
		.toSorted((a, b) => Number(b.isDefault) - Number(a.isDefault))
		.filter(
			(model) =>
				!wanted || `${model.label} ${model.id} ${model.description}`.toLowerCase().includes(wanted),
		);
	const shown = all || wanted ? items : items.slice(0, FIRST);

	return (
		<div className="space-y-2">
			{models.data.items.length >= FILTER_FROM && (
				<label className="border-border focus-within:ring-ring flex h-7 items-center gap-1.5 rounded-md border px-2 text-xs focus-within:ring-2">
					<Search className="text-muted-foreground size-3.5" />
					<span className="sr-only">{t("agentModels.filter")}</span>
					<input
						value={query}
						onChange={(event) => setQuery(event.target.value)}
						placeholder={t("agentModels.filterPlaceholder", { count: models.data.items.length })}
						className="placeholder:text-muted-foreground min-w-0 flex-1 bg-transparent outline-none"
					/>
				</label>
			)}
			{items.length === 0 ? (
				<p className="text-muted-foreground text-xs">
					{wanted ? t("agentModels.noMatch", { query }) : t("agentModels.none")}
				</p>
			) : (
				<ul className="border-border divide-border divide-y rounded-md border">
					{shown.map((model) => (
						<ModelRow key={model.id} model={model} />
					))}
				</ul>
			)}
			<div className="text-muted-foreground flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
				{!wanted && items.length > FIRST && (
					<button
						type="button"
						onClick={() => setAll(!all)}
						className="hover:text-foreground underline underline-offset-4"
					>
						{all ? t("agentModels.showFewer") : t("agentModels.showAll", { count: items.length })}
					</button>
				)}
				<span
					className="ml-auto"
					title={formatDate(models.data.fetchedAt, {
						year: "numeric",
						month: "numeric",
						day: "numeric",
						hour: "numeric",
						minute: "numeric",
						second: "numeric",
					})}
				>
					{models.data.refreshing
						? t("agentModels.rereading")
						: t("agentModels.readAgo", { ago: agoInWords(models.data.fetchedAt) })}
				</span>
				<ReadAgain pending={refresh.isPending} onClick={() => refresh.mutate()} />
			</div>
			{refresh.isError && (
				<p role="alert" className="text-destructive text-xs">
					{t("agentModels.cannotReread", { error: refresh.error.message })}
				</p>
			)}
		</div>
	);
}

function ModelRow({ model }: { model: AgentModel }) {
	const { t } = useTranslation("sessions");
	return (
		<li className="space-y-1 px-3 py-2">
			<div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
				<span className="text-heading text-sm font-medium">{model.label}</span>
				{model.isDefault && <span className="pill pill-ok">{t("agentModels.default")}</span>}
				<span className="text-muted-foreground ml-auto font-mono text-xs">{model.id}</span>
			</div>
			{model.description && model.description !== model.label && (
				<p className="text-muted-foreground line-clamp-2 text-xs">{model.description}</p>
			)}
			{model.efforts.length > 0 && (
				<ul className="flex flex-wrap gap-1" aria-label={t("agentModels.efforts")}>
					{model.efforts.map((effort) => {
						const preset = effort.id === model.defaultEffort;
						return (
							<li
								key={effort.id}
								title={effort.description || undefined}
								className={cn(
									"rounded px-1.5 py-px font-mono text-xs",
									preset ? "bg-primary/15 text-heading" : "bg-muted text-muted-foreground",
								)}
							>
								{effort.id}
								{preset && <span className="sr-only">{t("agentModels.defaultEffort")}</span>}
							</li>
						);
					})}
				</ul>
			)}
		</li>
	);
}

function ReadAgain({ pending, onClick }: { pending: boolean; onClick: () => void }) {
	const { t } = useTranslation("sessions");
	return (
		<Button variant="ghost" size="xs" disabled={pending} onClick={onClick}>
			<RotateCw className={pending ? "motion-safe:animate-spin" : undefined} />
			{pending ? t("agentModels.reading") : t("agentModels.readAgain")}
		</Button>
	);
}
