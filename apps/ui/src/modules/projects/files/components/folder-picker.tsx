import { cn } from "cn";
import { ArrowUp, Folder, GitBranch } from "lucide-react";
import { useState } from "react";

import { useFolders, useHomeFolder } from "@/modules/projects/files/api";
import { Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

export interface PickerMachine {
	id: string;
	name: string;
	online: boolean;
}

/**
 * Choose a folder on a machine: browse from its home folder, folders only.
 * With one machine, no machine is asked for.
 */
export function FolderPicker({
	machines,
	action,
	pending,
	error,
	onPick,
}: {
	machines: PickerMachine[];
	/** The button's words, such as "Open folder". */
	action: string;
	pending?: boolean;
	error?: string;
	onPick: (machine: string, path: string) => void;
}) {
	const { t } = useTranslation("projects");
	const online = machines.filter((machine) => machine.online);
	const [machine, setMachine] = useState(() => machines.find((item) => item.online)?.id);
	const [chosen, setPath] = useState<string>();
	const [hidden, setHidden] = useState(false);
	const home = useHomeFolder(machine);
	// Until a folder is chosen, the machine's home folder.
	const path = chosen ?? home.data?.path;
	const folders = useFolders(machine, path, hidden);
	const name = path?.replace(/\/+$/, "").split("/").pop();

	if (online.length === 0)
		return (
			<p className="text-muted-foreground text-sm">
				<Trans
					t={t}
					i18nKey="folderPicker.noMachine"
					components={{ code: <code className="font-mono" /> }}
				/>
			</p>
		);

	return (
		<div className="flex flex-col gap-3">
			{online.length > 1 ? (
				<label className="flex items-center gap-2 text-sm">
					<span className="text-muted-foreground">{t("folderPicker.machine")}</span>
					<select
						value={machine}
						onChange={(event) => {
							setMachine(event.target.value);
							setPath(undefined);
						}}
						className="border-input bg-background focus-visible:ring-ring h-8 rounded-md border px-2 text-sm outline-none focus-visible:ring-2"
					>
						{online.map((item) => (
							<option key={item.id} value={item.id}>
								{item.name}
							</option>
						))}
					</select>
				</label>
			) : null}
			<div className="flex items-center gap-2">
				<button
					type="button"
					aria-label={t("folderPicker.up")}
					disabled={!folders.data?.parent}
					onClick={() => folders.data?.parent && setPath(folders.data.parent)}
					className="border-border hover:bg-accent focus-visible:ring-ring rounded-md border p-1.5 outline-none focus-visible:ring-2 disabled:opacity-40"
				>
					<ArrowUp className="size-4" />
				</button>
				<p
					className="bg-muted min-w-0 flex-1 truncate rounded-md px-2.5 py-1.5 font-mono text-xs"
					title={path}
				>
					{path ?? "…"}
				</p>
			</div>
			<ul
				aria-label={t("folderPicker.folders")}
				className="border-border h-72 overflow-auto rounded-lg border py-1"
			>
				{folders.isPending ? (
					<li className="text-muted-foreground px-3 py-2 text-sm">
						{t("state.loading", { ns: "common" })}
					</li>
				) : folders.isError ? (
					<li className="text-muted-foreground px-3 py-2 text-sm">
						{t("folderPicker.cannotList")}
					</li>
				) : folders.data.folders.length === 0 ? (
					<li className="text-muted-foreground px-3 py-2 text-sm">{t("folderPicker.empty")}</li>
				) : (
					folders.data.folders.map((folder) => (
						<li key={folder.path}>
							<button
								type="button"
								onClick={() => setPath(folder.path)}
								className={cn(
									"hover:bg-accent focus-visible:bg-accent flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm outline-none",
									folder.hidden && "text-muted-foreground",
								)}
							>
								<Folder className="text-muted-foreground size-4 shrink-0" />
								<span className="truncate">{folder.name}</span>
								{folder.isGitRepo ? (
									<span className="text-muted-foreground ml-auto flex items-center gap-1 text-xs">
										<GitBranch className="size-3" />
										git
									</span>
								) : null}
							</button>
						</li>
					))
				)}
			</ul>
			<div className="flex items-center gap-3">
				<label className="text-muted-foreground flex items-center gap-2 text-sm">
					<input
						type="checkbox"
						checked={hidden}
						onChange={(event) => setHidden(event.target.checked)}
						className="accent-[var(--primary)]"
					/>
					{t("folderPicker.showHidden")}
				</label>
				<Button
					className="ml-auto"
					disabled={!machine || !path || pending}
					onClick={() => machine && path && onPick(machine, path)}
				>
					{pending
						? t("folderPicker.opening")
						: name
							? t("folderPicker.pickNamed", { action, name })
							: action}
				</Button>
			</div>
			{error ? (
				<p role="alert" className="text-destructive text-sm">
					{error}
				</p>
			) : null}
		</div>
	);
}
