import { ChevronDown, ChevronRight, Plus } from "lucide-react";
import { type FormEvent, useState } from "react";

import { useAddAgent, useAgentCatalog, useRemoveAgent } from "@/modules/machines/agents/api";
import { AgentIcon } from "@/modules/machines/agents/components/agent-icon";
import type { CatalogAgent, Machine } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";
import { toast } from "@/shared/ui/toast";

/** Whether `query` names a catalog agent: its name, ID or description. */
function matches(agent: CatalogAgent, query: string) {
	const wanted = query.trim().toLowerCase();
	return !wanted || `${agent.name} ${agent.id} ${agent.description}`.toLowerCase().includes(wanted);
}

/**
 * The ACP registry's agents not yet on a machine, folded: each with what it
 * is, how it is launched, and Add (npx and uvx agents; those shipped only as
 * downloads come later).
 */
export function AvailableAgents({
	machine,
	present,
	query,
	folded,
}: {
	machine: Machine;
	/** Agents already on the machine, by ID. */
	present: ReadonlySet<string>;
	query: string;
	folded: boolean;
}) {
	const { t } = useTranslation("machines");
	const catalog = useAgentCatalog();
	const [open, setOpen] = useState<boolean | undefined>(undefined);
	const available = (catalog.data?.items ?? []).filter(
		(agent) => agent.source === "registry" && !present.has(agent.id) && matches(agent, query),
	);
	if (available.length === 0) return null;
	const shown = open ?? !folded;
	return (
		<section aria-label={t("catalog.title")} className="space-y-1">
			<h3 className="flex flex-wrap items-baseline gap-x-2">
				<button
					type="button"
					aria-expanded={shown}
					onClick={() => setOpen(!shown)}
					className="text-muted-foreground hover:text-foreground flex items-center gap-1 text-xs font-medium tracking-wide uppercase"
				>
					{shown ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
					{t("catalog.title")} <span className="tabular-nums">({available.length})</span>
				</button>
				{catalog.data && (
					<span className="text-muted-foreground text-[0.6875rem]">
						{t(
							catalog.data.registrySource === "bundled"
								? "catalog.registryBundled"
								: "catalog.registry",
							{ version: catalog.data.registryVersion },
						)}
					</span>
				)}
			</h3>
			{shown && (
				<ul className="border-border divide-border divide-y border-y">
					{available.map((agent) => (
						<AvailableRow key={agent.id} machine={machine} agent={agent} />
					))}
				</ul>
			)}
		</section>
	);
}

function AvailableRow({ machine, agent }: { machine: Machine; agent: CatalogAgent }) {
	const { t } = useTranslation("machines");
	const add = useAddAgent(machine.id);
	return (
		<li className="flex min-h-11 items-center gap-3 px-2 py-1.5 text-sm">
			<AgentIcon agent={agent.id} className="opacity-70" />
			<span className="min-w-0 flex-1">
				<span className="flex flex-wrap items-baseline gap-x-2">
					<span className="text-foreground font-medium">{agent.name}</span>
					{agent.version && (
						<span className="text-muted-foreground font-mono text-xs">{agent.version}</span>
					)}
				</span>
				<span className="text-muted-foreground block truncate text-xs" title={agent.description}>
					{agent.description}
				</span>
			</span>
			<span className="text-muted-foreground hidden text-[0.6875rem] sm:inline">
				{agent.methods.join(" · ")}
			</span>
			{agent.addable ? (
				<Button
					size="xs"
					variant="outline"
					disabled={add.isPending}
					onClick={() =>
						add.mutate(
							{ id: machine.id, data: { agent: agent.id, args: [], env: {} } },
							{
								onSuccess: () =>
									toast(t("catalog.added", { agent: agent.name, machine: machine.name })),
								onError: (error) =>
									toast(t("catalog.addFailed", { agent: agent.name, message: error.message }), {
										duration: 8000,
									}),
							},
						)
					}
				>
					{add.isPending ? t("catalog.adding") : t("catalog.add")}
				</Button>
			) : (
				<span className="text-muted-foreground text-xs" title={t("catalog.downloadOnlyTitle")}>
					{t("catalog.downloadOnly")}
				</span>
			)}
		</li>
	);
}

/** “+ Add custom agent”: any program that speaks the Agent Client Protocol. */
export function AddCustomAgent({ machine }: { machine: Machine }) {
	const { t } = useTranslation("machines");
	const { t: common } = useTranslation();
	const [open, setOpen] = useState(false);
	const [name, setName] = useState("");
	const [program, setProgram] = useState("");
	const [args, setArgs] = useState("");
	const add = useAddAgent(machine.id);

	const submit = (event: FormEvent) => {
		event.preventDefault();
		add.mutate(
			{
				id: machine.id,
				data: {
					name,
					program,
					args: args.split(/\s+/).filter(Boolean),
					env: {},
				},
			},
			{
				onSuccess: (added) => {
					setOpen(false);
					setName("");
					setProgram("");
					setArgs("");
					toast(t("addCustomAgent.added", { agent: added.name, machine: machine.name }));
				},
			},
		);
	};

	return (
		<>
			<Button variant="outline" size="xs" onClick={() => setOpen(true)}>
				<Plus />
				{t("addCustomAgent.button")}
			</Button>
			<Dialog open={open} onOpenChange={setOpen}>
				<DialogContent size="md">
					<DialogHeader>
						<DialogTitle>{t("addCustomAgent.title", { machine: machine.name })}</DialogTitle>
						<DialogDescription>
							{t("addCustomAgent.description", { machine: machine.name })}
						</DialogDescription>
					</DialogHeader>
					<form onSubmit={submit} className="flex min-h-0 flex-col text-sm">
						<DialogBody className="gap-3">
							<Field
								label={t("addCustomAgent.name")}
								value={name}
								onChange={setName}
								placeholder={t("addCustomAgent.namePlaceholder")}
							/>
							<Field
								label={t("addCustomAgent.program")}
								value={program}
								onChange={setProgram}
								placeholder={t("addCustomAgent.programPlaceholder")}
								mono
							/>
							<Field
								label={t("addCustomAgent.arguments")}
								value={args}
								onChange={setArgs}
								placeholder="--acp"
								mono
							/>
							{add.isError && (
								<p role="alert" className="text-destructive text-xs">
									{add.error.message}
								</p>
							)}
						</DialogBody>
						<DialogFooter>
							<Button type="button" variant="ghost" onClick={() => setOpen(false)}>
								{common("actions.cancel")}
							</Button>
							<Button type="submit" disabled={!name.trim() || !program.trim() || add.isPending}>
								{add.isPending ? t("addCustomAgent.adding") : t("addCustomAgent.add")}
							</Button>
						</DialogFooter>
					</form>
				</DialogContent>
			</Dialog>
		</>
	);
}

function Field({
	label,
	value,
	onChange,
	placeholder,
	mono = false,
}: {
	label: string;
	value: string;
	onChange: (value: string) => void;
	placeholder?: string;
	mono?: boolean;
}) {
	return (
		<label className="block space-y-1">
			<span className="text-muted-foreground text-xs font-medium">{label}</span>
			<input
				value={value}
				onChange={(event) => onChange(event.target.value)}
				placeholder={placeholder}
				className={`border-border bg-background focus-visible:ring-ring h-8 w-full rounded-md border px-2.5 outline-none focus-visible:ring-2 ${mono ? "font-mono text-xs" : "text-sm"}`}
			/>
		</label>
	);
}

/** Removes an added agent from the machine, after asking. */
export function RemoveAgentButton({
	machine,
	agent,
	name,
	onRemoved,
}: {
	machine: Machine;
	agent: string;
	name: string;
	onRemoved: () => void;
}) {
	const { t } = useTranslation("machines");
	const { t: common } = useTranslation();
	const [asking, setAsking] = useState(false);
	const remove = useRemoveAgent(machine.id);
	return (
		<>
			<Button variant="outline" size="xs" onClick={() => setAsking(true)}>
				{t("removeAgent.button", { machine: machine.name })}
			</Button>
			<ConfirmDialog
				open={asking}
				onOpenChange={setAsking}
				title={t("removeAgent.title", { agent: name, machine: machine.name })}
				description={t("removeAgent.description", { agent: name, machine: machine.name })}
				action={common("actions.remove")}
				destructive
				pending={remove.isPending}
				error={remove.error?.message}
				onConfirm={() =>
					remove.mutate(
						{ id: machine.id, agent },
						{
							onSuccess: () => {
								setAsking(false);
								onRemoved();
							},
						},
					)
				}
			/>
		</>
	);
}
