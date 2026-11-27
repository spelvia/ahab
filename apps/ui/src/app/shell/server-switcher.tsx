import { AppWindow, Check, ChevronDown, Plus, Server } from "lucide-react";

import { app } from "@/shared/desktop/bridge";
import { useAppServers, useDesktopApp } from "@/shared/desktop/hooks";
import { useTranslation } from "@/shared/i18n";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

/**
 * In the desktop app: the servers it is paired with, to switch
 * this window to another, add one, or open a new window. Nothing in a
 * browser.
 */
export function ServerSwitcher({ path }: { path: string }) {
	const { t } = useTranslation("shell");
	const desktop = useDesktopApp();
	const servers = useAppServers(Boolean(desktop));
	if (!desktop) return null;
	const list = servers.data ?? [];
	const current = list.find((server) => server.current)?.name ?? desktop.server.name;
	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				className="text-muted-foreground hover:text-foreground hover:bg-accent data-[state=open]:bg-accent focus-visible:ring-ring flex h-7 max-w-40 items-center gap-1.5 rounded-md px-1.5 text-sm outline-none focus-visible:ring-2"
				aria-label={t("serverSwitcher.label")}
			>
				<Server className="size-3.5 shrink-0" />
				<span className="truncate">{current}</span>
				<ChevronDown className="size-3.5 shrink-0" />
			</DropdownMenuTrigger>
			<DropdownMenuContent align="start" className="w-64">
				{list.map((server) => (
					<DropdownMenuItem
						key={server.id}
						onSelect={() => {
							if (!server.current) void app.switchServer(server.id);
						}}
					>
						<span className="min-w-0 flex-1">
							<span className="text-heading block truncate">{server.name}</span>
							<span className="text-muted-foreground block truncate font-mono text-xs">
								{server.url}
							</span>
						</span>
						{server.current ? <Check /> : null}
					</DropdownMenuItem>
				))}
				<DropdownMenuSeparator />
				<DropdownMenuItem onSelect={() => void app.newWindow(path)}>
					<AppWindow />
					{t("serverSwitcher.newWindow")}
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={() => void app.addServer()}>
					<Plus />
					{t("serverSwitcher.add")}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
