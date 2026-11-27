import { type ReactNode, useState } from "react";

import { kindLabel } from "@/modules/tools/registry";
import { envToText, parseEnv, withTool } from "@/modules/tools/setting/model";
import type { ToolSettings, ToolStatusDto } from "@/shared/api/generated/ahab.schemas";
import { Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { CopyButton } from "@/shared/ui/copy-button";
import { Drawer, DrawerContent } from "@/shared/ui/drawer";
import { Switch } from "@/shared/ui/switch";

const INPUT =
	"border-input bg-background focus-visible:ring-ring h-8 w-full rounded-md border px-2 font-mono text-xs outline-none focus-visible:ring-2";

/**
 * One tool on a machine: what it is and where it was found, and this
 * machine's settings for it (its path, extra environment, off here).
 * A tool of your own can be edited or removed.
 */
export function ToolDrawer({
	machineName,
	tool,
	settings,
	pending,
	error,
	onSave,
	onEditRecord,
	onRemoveRecord,
	onClose,
}: {
	machineName: string;
	tool: ToolStatusDto | undefined;
	settings: ToolSettings;
	pending?: boolean;
	error?: string;
	onSave: (settings: ToolSettings) => void;
	onEditRecord: (id: string) => void;
	onRemoveRecord: (id: string) => void;
	onClose: () => void;
}) {
	return (
		<Drawer open={Boolean(tool)} onOpenChange={(open) => !open && onClose()}>
			{tool && (
				<DrawerContent title={tool.name} description={kindLabel(tool.kind)}>
					<Details
						key={tool.id}
						machineName={machineName}
						tool={tool}
						settings={settings}
						pending={pending}
						error={error}
						onSave={onSave}
						onEditRecord={onEditRecord}
						onRemoveRecord={onRemoveRecord}
					/>
				</DrawerContent>
			)}
		</Drawer>
	);
}

function Details({
	machineName,
	tool,
	settings,
	pending,
	error,
	onSave,
	onEditRecord,
	onRemoveRecord,
}: {
	machineName: string;
	tool: ToolStatusDto;
	settings: ToolSettings;
	pending?: boolean;
	error?: string;
	onSave: (settings: ToolSettings) => void;
	onEditRecord: (id: string) => void;
	onRemoveRecord: (id: string) => void;
}) {
	const { t } = useTranslation(["tools", "common"]);
	const choice = settings.tools?.[tool.id] ?? {};
	const [path, setPath] = useState(choice.path ?? "");
	const [env, setEnv] = useState(envToText(choice.env));
	const [errors, setErrors] = useState<string[]>([]);
	const off = Boolean(choice.off);
	const own = tool.origin === "machine";

	const save = () => {
		const parsed = parseEnv(env);
		setErrors(parsed.errors);
		if (parsed.errors.length > 0) return;
		onSave(withTool(settings, tool.id, { ...choice, path: path.trim() || null, env: parsed.env }));
	};

	return (
		<div className="space-y-6 text-sm">
			{tool.description && <p>{tool.description}</p>}
			{tool.state === "missing" && tool.hint && (
				<p className="border-warning/45 bg-warning/10 rounded-md border px-3 py-2">
					<Trans
						t={t}
						i18nKey="toolDrawer.notFound"
						values={{ machine: machineName, hint: tool.hint }}
						components={{ code: <code className="font-mono text-xs" /> }}
					/>
				</p>
			)}
			{tool.state === "cannotRun" && tool.reason && (
				<p className="border-warning/45 bg-warning/10 rounded-md border px-3 py-2">{tool.reason}</p>
			)}

			<Section title={t("toolDrawer.about")}>
				<dl className="divide-border divide-y">
					{tool.path && (
						<Item label={t("toolDrawer.program")}>
							<span className="flex min-w-0 items-center gap-1">
								<span className="truncate font-mono text-xs" title={tool.path}>
									{tool.path}
								</span>
								<CopyButton text={tool.path} label={t("toolDrawer.copyPath")} />
							</span>
						</Item>
					)}
					{tool.detail && <Item label={t("toolDrawer.found")}>{tool.detail}</Item>}
					{(tool.foundVersion || tool.version) && (
						<Item label={t("toolDrawer.version")}>
							<span className="font-mono text-xs">
								{tool.version && tool.version !== tool.foundVersion
									? t("toolDrawer.versionWithAhab", {
											found: tool.foundVersion ?? "—",
											version: tool.version,
										})
									: (tool.foundVersion ?? "—")}
							</span>
						</Item>
					)}
					{tool.source && <Item label={t("toolDrawer.source")}>{tool.source}</Item>}
					<Item label={t("toolDrawer.from")}>
						{tool.origin === "ahab"
							? tool.default
								? t("toolDrawer.fromAhabDefault")
								: t("toolDrawer.fromAhab")
							: own
								? t("toolDrawer.fromMachine")
								: t("toolDrawer.fromPlugin", { plugin: tool.origin })}
					</Item>
					{tool.license && <Item label={t("toolDrawer.licence")}>{tool.license}</Item>}
					{tool.homepage && (
						<Item label={t("toolDrawer.homePage")}>
							<a
								href={tool.homepage}
								target="_blank"
								rel="noreferrer"
								className="underline underline-offset-4"
							>
								{tool.homepage.replace(/^https?:\/\//, "")}
							</a>
						</Item>
					)}
				</dl>
				{own && (
					<div className="flex gap-2 pt-2">
						<Button size="sm" variant="outline" onClick={() => onEditRecord(tool.id)}>
							{t("toolDrawer.edit")}
						</Button>
						<Button size="sm" variant="ghost" onClick={() => onRemoveRecord(tool.id)}>
							{t("toolDrawer.removeRecord")}
						</Button>
					</div>
				)}
			</Section>

			<Section title={t("toolDrawer.onMachine", { machine: machineName })}>
				<label className="flex items-center justify-between gap-4 py-1">
					<span>{t("toolDrawer.turnOff")}</span>
					<Switch
						label={t("toolDrawer.turnOff")}
						checked={off}
						disabled={pending}
						onCheckedChange={(next) =>
							onSave(withTool(settings, tool.id, { ...choice, off: next || null }))
						}
					/>
				</label>
				<label className="block space-y-1">
					<span className="text-muted-foreground text-xs">{t("toolDrawer.path")}</span>
					<input
						className={INPUT}
						value={path}
						placeholder="/opt/tools/bin/tool"
						onChange={(event) => setPath(event.target.value)}
					/>
				</label>
				<label className="block space-y-1">
					<span className="text-muted-foreground text-xs">{t("toolDrawer.environment")}</span>
					<textarea
						className={`${INPUT} h-auto py-1.5`}
						rows={3}
						value={env}
						placeholder="JAVA_HOME=/usr/lib/jvm/java-21"
						spellCheck={false}
						onChange={(event) => setEnv(event.target.value)}
					/>
				</label>
				{[...errors, ...(error ? [error] : [])].map((message) => (
					<p key={message} role="alert" className="text-destructive text-xs">
						{message}
					</p>
				))}
				<div className="flex justify-end">
					<Button size="sm" disabled={pending} onClick={save}>
						{pending ? t("common:actions.saving") : t("common:actions.save")}
					</Button>
				</div>
			</Section>
		</div>
	);
}

function Section({ title, children }: { title: string; children: ReactNode }) {
	return (
		<section className="space-y-2">
			<h3 className="text-muted-foreground text-xs font-medium tracking-wide uppercase">{title}</h3>
			{children}
		</section>
	);
}

function Item({ label, children }: { label: string; children: ReactNode }) {
	return (
		<div className="grid gap-1 py-2 sm:grid-cols-[6rem_minmax(0,1fr)] sm:gap-3">
			<dt className="text-muted-foreground">{label}</dt>
			<dd className="min-w-0">{children}</dd>
		</div>
	);
}
