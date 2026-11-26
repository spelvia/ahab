import { Files, Search } from "lucide-react";

import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/**
 * What an empty tile area shows: the current project and how to start.
 */
export function StartSurface({
	projectName,
	machineName,
	path,
	onBrowse,
	onCommand,
	commandKey,
}: {
	projectName?: string;
	machineName?: string;
	path?: string;
	onBrowse: () => void;
	/** The palette's key, as shown. */
	commandKey?: string;
	onCommand: () => void;
}) {
	const { t } = useTranslation("projects");
	return (
		<div className="flex h-full items-center justify-center p-8">
			<div className="w-full max-w-md">
				{projectName ? (
					<>
						<p className="text-muted-foreground text-xs font-medium tracking-wide uppercase">
							{machineName}
						</p>
						<h2 className="font-heading text-heading mt-1 text-xl font-semibold">{projectName}</h2>
						<p className="text-muted-foreground mt-1 truncate font-mono text-xs" title={path}>
							{path}
						</p>
					</>
				) : (
					<h2 className="font-heading text-heading text-xl font-semibold">
						{t("startSurface.noProject")}
					</h2>
				)}
				<div className="mt-6 flex flex-wrap gap-2">
					<Button onClick={onBrowse} disabled={!projectName}>
						<Files />
						{t("startSurface.browseFiles")}
					</Button>
					<Button variant="outline" onClick={onCommand}>
						<Search />
						{t("startSurface.commands")}
						{commandKey ? (
							<kbd className="text-muted-foreground ml-1 font-mono text-[0.6875rem]">
								{commandKey}
							</kbd>
						) : null}
					</Button>
				</div>
				<p className="text-muted-foreground mt-6 text-sm">{t("startSurface.comingSoon")}</p>
			</div>
		</div>
	);
}
