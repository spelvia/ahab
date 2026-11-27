import { cn } from "cn";
import { AppWindow, Check, ChevronDown, FolderOpen, GripVertical, Settings2 } from "lucide-react";
import { Fragment, useState } from "react";

import { useSetProjectOrder } from "@/modules/projects/project/api";
import {
	groupProjects,
	inListOrder,
	projectPlace,
	reordered,
	sharedNames,
} from "@/modules/projects/project/model";
import type { Project, ProjectIcon } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

/**
 * A project's mark: its own emoji or image when it has one, else its
 * initial on a tile of its own colour, so it is easy to spot.
 */
export function ProjectMark({
	name,
	icon,
	className,
}: {
	name: string;
	icon?: ProjectIcon | null;
	className?: string;
}) {
	const box = "grid size-5 shrink-0 place-items-center overflow-hidden rounded-[5px]";
	if (icon?.image)
		return (
			<img
				src={icon.image}
				alt=""
				aria-hidden
				className={cn(box, "bg-muted object-cover", className)}
			/>
		);
	if (icon?.emoji)
		return (
			<span aria-hidden className={cn(box, "text-[0.8125rem] leading-none", className)}>
				{icon.emoji}
			</span>
		);
	const hue = [...name].reduce((sum, char) => sum + char.charCodeAt(0), 0) % 360;
	return (
		<span
			aria-hidden
			className={cn(box, "font-heading text-[0.6875rem] font-semibold text-[#17201c]", className)}
			style={{ background: `oklch(0.78 0.07 ${hue})` }}
		>
			{name.charAt(0).toUpperCase()}
		</span>
	);
}

const NONE_WAITING: Record<string, number> = {};

/**
 * Where a project is, in a line: its machine, or its folder on its machine
 * when another listed project has the same name.
 */
function summary(project: Project, shared: Set<string>): string {
	return shared.has(project.id) ? projectPlace(project) : project.machineName;
}

/** The current project in the title bar; opens the list of projects. */
export function ProjectSwitcher({
	projects,
	current,
	onSwitch,
	onOpenFolder,
	onManage,
	onOpenInNewWindow,
	shortcut,
	waiting = NONE_WAITING,
}: {
	projects: Project[];
	current?: Project;
	/** Sessions that need you, per project ID. */
	waiting?: Record<string, number>;
	onSwitch: (project: Project) => void;
	onOpenFolder: () => void;
	onManage: () => void;
	/** In the desktop app: the current project in another window. */
	onOpenInNewWindow?: () => void;
	/** The key that switches to the project at this place (0 is the first), if any. */
	shortcut?: (index: number) => string | undefined;
}) {
	const { t } = useTranslation("projects");
	const shared = sharedNames(projects);
	const order = useSetProjectOrder();
	const [dragging, setDragging] = useState<string>();
	const [over, setOver] = useState<string>();
	const place = new Map(inListOrder(projects).map((project, index) => [project.id, index]));
	const move = (id: string, target: string | -1 | 1) => {
		const ids = reordered(projects, id, target);
		if (ids) order.mutate({ data: { ids } });
	};
	const keys = projects.length
		? [shortcut?.(0), shortcut?.(Math.min(projects.length, 9) - 1)].filter(Boolean)
		: [];
	const elsewhere = projects.some(
		(project) => project.id !== current?.id && (waiting[project.id] ?? 0) > 0,
	);
	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				className="hover:bg-accent data-[state=open]:bg-accent focus-visible:ring-ring flex h-7 max-w-64 items-center gap-2 rounded-md px-1.5 text-sm outline-none focus-visible:ring-2"
				aria-label={t("projectSwitcher.label")}
			>
				{current ? <ProjectMark name={current.name} icon={current.icon} /> : null}
				<span className="text-heading truncate font-semibold">
					{current?.name ?? t("projectSwitcher.projects")}
				</span>
				<ChevronDown className="text-muted-foreground size-3.5 shrink-0" />
				{elsewhere ? (
					<span
						className="bg-warning size-1.5 shrink-0 rounded-full"
						title={t("projectSwitcher.elsewhereWaiting")}
					/>
				) : null}
			</DropdownMenuTrigger>
			<DropdownMenuContent align="start" className="w-80">
				{groupProjects(projects).map(({ group, projects: members }, index) => (
					<Fragment key={group ?? ""}>
						{group !== null || index > 0 ? (
							<p className="text-muted-foreground px-2 pt-2 pb-1 text-xs font-medium">
								{group ?? t("projectSwitcher.ungrouped")}
							</p>
						) : null}
						{members.map((project) => (
							<DropdownMenuItem
								key={project.id}
								onSelect={() => onSwitch(project)}
								draggable={projects.length > 1}
								onDragStart={(event) => {
									event.dataTransfer.effectAllowed = "move";
									event.dataTransfer.setData("text/plain", project.id);
									setDragging(project.id);
								}}
								onDragEnd={() => {
									setDragging(undefined);
									setOver(undefined);
								}}
								onDragOver={(event) => {
									if (!dragging || !reordered(projects, dragging, project.id)) return;
									event.preventDefault();
									setOver(project.id);
								}}
								onDragLeave={() => setOver((now) => (now === project.id ? undefined : now))}
								onDrop={(event) => {
									event.preventDefault();
									if (dragging) move(dragging, project.id);
									setDragging(undefined);
									setOver(undefined);
								}}
								onKeyDown={(event) => {
									if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown"))
										return;
									event.preventDefault();
									event.stopPropagation();
									move(project.id, event.key === "ArrowUp" ? -1 : 1);
								}}
								aria-keyshortcuts={shortcut?.(place.get(project.id) ?? 9)}
								className={cn(
									"group/project gap-2 py-2 pl-1",
									dragging === project.id && "opacity-50",
									over === project.id && "ring-primary/60 ring-1 ring-inset",
								)}
							>
								<GripVertical
									aria-hidden
									className="text-muted-foreground/50 group-hover/project:text-muted-foreground -mr-1 size-3.5 shrink-0 cursor-grab"
								/>
								<ProjectMark name={project.name} icon={project.icon} />
								<span className="min-w-0 flex-1">
									<span className="text-heading block truncate font-medium">{project.name}</span>
									<span className="text-muted-foreground block truncate text-xs">
										{summary(project, shared)}
									</span>
								</span>
								{(waiting[project.id] ?? 0) > 0 ? (
									<span className="text-warning shrink-0 text-xs">
										{t("projectSwitcher.waiting", { count: waiting[project.id] })}
									</span>
								) : null}
								{project.id === current?.id ? <Check /> : null}
								{shortcut?.(place.get(project.id) ?? 9) ? (
									<kbd className="text-muted-foreground shrink-0 font-sans text-xs tabular-nums">
										{shortcut(place.get(project.id) ?? 9)}
									</kbd>
								) : null}
							</DropdownMenuItem>
						))}
					</Fragment>
				))}
				{projects.length > 1 ? (
					<p className="text-muted-foreground px-2 pt-1 pb-1.5 text-xs">
						{keys.length === 2
							? t("projectSwitcher.hint", { first: keys[0], last: keys[1] })
							: t("projectSwitcher.hintReorder")}
					</p>
				) : null}
				<DropdownMenuSeparator />
				<DropdownMenuItem onSelect={onOpenFolder}>
					<FolderOpen />
					{t("projectSwitcher.openFolder")}
				</DropdownMenuItem>
				{onOpenInNewWindow ? (
					<DropdownMenuItem onSelect={onOpenInNewWindow}>
						<AppWindow />
						{t("projectSwitcher.openInNewWindow")}
					</DropdownMenuItem>
				) : null}
				<DropdownMenuItem onSelect={onManage}>
					<Settings2 />
					{t("projectSwitcher.manage")}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
