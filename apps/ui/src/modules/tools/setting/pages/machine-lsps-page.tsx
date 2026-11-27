import { Plus } from "lucide-react";
import { useState } from "react";

import { MachineSection, useMachines } from "@/modules/machines";
import { MachineTools, machineProblem } from "@/modules/tools/registry";
import { useMachineToolSettings, useSaveMachineToolSettings } from "@/modules/tools/setting/api";
import { LanguageServers } from "@/modules/tools/setting/components/language-servers";
import { RecordDialog } from "@/modules/tools/setting/components/record-dialog";
import { ToolDrawer } from "@/modules/tools/setting/components/tool-drawer";
import { MAX_CHECKS, MAX_SERVERS, withoutRecord, withRecord } from "@/modules/tools/setting/model";
import { problemCode } from "@/shared/api/fetch";
import type {
	Machine,
	ToolSettings,
	ToolSettingsRecordsItem,
	ToolStatusDto,
} from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { Switch } from "@/shared/ui/switch";

/**
 * A machine's LSPs tab: its tools and runtimes, the language
 * servers running there, and what is set for this machine alone.
 */
export function MachineLspsPage({ id }: { id: string }) {
	const machines = useMachines();
	const machine = machines.data?.items.find((item) => item.id === id);
	if (!machine) return null;
	return <Loaded machine={machine} />;
}

function Loaded({ machine }: { machine: Machine }) {
	const { t } = useTranslation("tools");
	const settings = useMachineToolSettings(machine.id);
	const saving = useSaveMachineToolSettings(machine.id);
	const [open, setOpen] = useState<ToolStatusDto>();
	const [editing, setEditing] = useState<{ record?: ToolSettingsRecordsItem }>();
	const current: ToolSettings = settings.data ?? {};
	const error = saving.error?.message;

	const saveRecord = (record: ToolSettingsRecordsItem) =>
		saving.save(withRecord(current, record, editing?.record?.id as string | undefined), {
			onSuccess: () => {
				setEditing(undefined);
				setOpen(undefined);
			},
		});

	return (
		<div className="space-y-10">
			<MachineSection
				id="tools"
				title={t("machineLsps.tools.title")}
				description={t("machineLsps.tools.description")}
			>
				<MachineTools
					machine={machine}
					onOpen={setOpen}
					actions={
						<Button
							variant="ghost"
							size="xs"
							disabled={!settings.data}
							onClick={() => setEditing({})}
						>
							<Plus />
							{t("machineLsps.tools.add")}
						</Button>
					}
				/>
			</MachineSection>

			<MachineSection
				id="servers"
				title={t("machineLsps.servers.title")}
				description={t("machineLsps.servers.description")}
			>
				<LanguageServers machine={machine} />
			</MachineSection>

			<MachineSection
				id="tool-settings"
				title={t("machineLsps.settings.title")}
				description={t("machineLsps.settings.description")}
			>
				{settings.isError && (
					<p role="alert" className="text-destructive text-sm">
						{machineProblem(problemCode(settings.error), settings.error.message)}
					</p>
				)}
				{settings.data && (
					<Limits
						settings={current}
						pending={saving.isPending}
						onSave={(next) => saving.save(next)}
					/>
				)}
				{error && !open && !editing && (
					<p role="alert" className="text-destructive text-sm">
						{error}
					</p>
				)}
			</MachineSection>

			<ToolDrawer
				machineName={machine.name}
				tool={open}
				settings={current}
				pending={saving.isPending}
				error={open ? error : undefined}
				onSave={(next) => saving.save(next)}
				onEditRecord={(tool) =>
					setEditing({ record: current.records?.find((record) => record.id === tool) })
				}
				onRemoveRecord={(tool) =>
					saving.save(withoutRecord(current, tool), { onSuccess: () => setOpen(undefined) })
				}
				onClose={() => setOpen(undefined)}
			/>
			<RecordDialog
				open={Boolean(editing)}
				onOpenChange={(next) => !next && setEditing(undefined)}
				record={editing?.record}
				pending={saving.isPending}
				error={editing ? error : undefined}
				onSave={saveRecord}
			/>
		</div>
	);
}

/**
 * How many servers and checks run at once, the servers' memory budget, and
 * whether their LSP traffic is kept in their logs.
 */
function Limits({
	settings,
	pending,
	onSave,
}: {
	settings: ToolSettings;
	pending: boolean;
	onSave: (settings: ToolSettings) => void;
}) {
	const { t } = useTranslation("tools");
	return (
		<ul className="border-border divide-border divide-y border-y text-sm">
			<NumberSetting
				id="max-servers"
				label={t("machineLsps.maxServers.label")}
				help={t("machineLsps.maxServers.help", { value: MAX_SERVERS })}
				value={settings.maxServers}
				fallback={MAX_SERVERS}
				min={1}
				max={64}
				pending={pending}
				onSave={(value) => onSave({ ...settings, maxServers: value })}
			/>
			<NumberSetting
				id="max-checks"
				label={t("machineLsps.maxChecks.label")}
				help={t("machineLsps.maxChecks.help", { value: MAX_CHECKS })}
				value={settings.maxChecks}
				fallback={MAX_CHECKS}
				min={1}
				max={16}
				pending={pending}
				onSave={(value) => onSave({ ...settings, maxChecks: value })}
			/>
			<NumberSetting
				id="memory-budget"
				label={t("machineLsps.memoryBudget.label")}
				help={t("machineLsps.memoryBudget.help")}
				value={settings.memoryBudget}
				fallback={null}
				min={256}
				max={1 << 22}
				pending={pending}
				onSave={(value) => onSave({ ...settings, memoryBudget: value })}
			/>
			<li className="flex flex-wrap items-center justify-between gap-3 py-2.5">
				<span className="space-y-0.5">
					<span className="block">{t("machineLsps.logTraffic.label")}</span>
					<span className="text-muted-foreground block text-xs">
						{t("machineLsps.logTraffic.help")}
					</span>
				</span>
				<Switch
					label={t("machineLsps.logTraffic.label")}
					checked={Boolean(settings.logTraffic)}
					disabled={pending}
					onCheckedChange={(next) => onSave({ ...settings, logTraffic: next || null })}
				/>
			</li>
		</ul>
	);
}

/** A whole number setting with its default: saved as none when it is the default (or empty). */
function NumberSetting({
	id,
	label,
	help,
	value,
	fallback,
	min,
	max,
	pending,
	onSave,
}: {
	id: string;
	label: string;
	help: string;
	value?: number | null;
	/** The default; `null` when the field may be empty. */
	fallback: number | null;
	min: number;
	max: number;
	pending: boolean;
	onSave: (value: number | null) => void;
}) {
	const [text, setText] = useState(String(value ?? fallback ?? ""));
	const parsed = text.trim() === "" ? null : Number.parseInt(text, 10);
	const valid =
		parsed === null
			? fallback === null
			: Number.isInteger(parsed) && parsed >= min && parsed <= max;
	const saved = parsed === fallback ? null : parsed;
	const changed = valid && saved !== (value ?? null);
	const { t } = useTranslation(["tools", "common"]);
	return (
		<li className="flex flex-wrap items-center justify-between gap-3 py-2.5">
			<label htmlFor={id} className="space-y-0.5">
				<span className="block">{label}</span>
				<span className="text-muted-foreground block text-xs">{help}</span>
			</label>
			<span className="flex items-center gap-2">
				<input
					id={id}
					type="number"
					min={min}
					max={max}
					value={text}
					placeholder={fallback === null ? t("machineLsps.memoryBudget.placeholder") : undefined}
					onChange={(event) => setText(event.target.value)}
					className="border-input bg-background focus-visible:ring-ring h-8 w-24 rounded-md border px-2 text-sm tabular-nums outline-none focus-visible:ring-2"
				/>
				{changed && (
					<Button size="sm" disabled={pending} onClick={() => onSave(saved)}>
						{t("common:actions.save")}
					</Button>
				)}
			</span>
		</li>
	);
}
