import { Link } from "@tanstack/react-router";
import { cn } from "cn";
import {
	ArrowDown,
	ArrowUp,
	CircleAlert,
	Folder,
	GitBranch,
	GitCommitVertical,
	type LucideIcon,
} from "lucide-react";
import type { ReactNode } from "react";

import { GLANCE, useGitStatus, useRepositories } from "@/modules/projects/git";
import { useProject } from "@/modules/projects/project/api";
import { projectState } from "@/modules/projects/project/model";
import type { Project } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/** Most repositories listed for a folder that holds several. */
const MOST_REPOSITORIES = 4;

/**
 * A project's Overview: the project at a glance, its machine, its Git and
 * (from `sessions`) what runs in it, with a way in.
 */
export function ProjectOverview({
	projectId,
	sessions,
}: {
	projectId: string;
	/** What runs in a project: its sessions, from the sessions module. */
	sessions?: (project: Project) => ReactNode;
}) {
	const project = useProject(projectId);
	const data = project.data;
	if (!data) return null;
	return (
		<div className="grid gap-3 md:grid-cols-2">
			<ProjectCard project={data}>{sessions?.(data)}</ProjectCard>
		</div>
	);
}

function ProjectCard({ project, children }: { project: Project; children?: ReactNode }) {
	const { t } = useTranslation("projects");
	const state = projectState(project);
	const available = project.availability === "available";
	return (
		<article
			aria-label={project.name}
			className="border-border bg-card flex flex-col gap-3 rounded-lg border p-4"
		>
			<header className="flex items-start gap-2.5">
				<Folder className="text-muted-foreground mt-0.5 size-4 shrink-0" aria-hidden />
				<div className="min-w-0 flex-1">
					<h2 className="text-heading truncate text-sm font-semibold" title={project.path}>
						{project.name}
					</h2>
					<p className="text-muted-foreground truncate font-mono text-xs" title={project.path}>
						{project.path}
					</p>
					<p className="text-muted-foreground flex items-center gap-1.5 text-xs">
						<span className={cn("size-1.5 rounded-full", state.dot)} aria-hidden />
						{state.label}
						{project.mode === "readOnly" ? (
							<span className="border-border rounded border px-1">
								{t("projectOverview.readOnly")}
							</span>
						) : null}
					</p>
				</div>
				<Button size="sm" variant="outline" asChild>
					<Link to="/p/$project" params={{ project: project.id }}>
						{t("projectOverview.open")}
					</Link>
				</Button>
			</header>
			{available ? (
				<GitGlance projectId={project.id} />
			) : (
				<p className="text-muted-foreground text-xs">
					{project.availability === "missing"
						? t("projectOverview.missing")
						: t("projectOverview.offline", { machine: project.machineName })}
				</p>
			)}
			{children}
		</article>
	);
}

/** Its Git: the branch and its changes, or each repository it holds. */
function GitGlance({ projectId }: { projectId: string }) {
	const { t } = useTranslation("projects");
	const found = useRepositories(projectId);
	if (found.isPending)
		return <Line icon={GitBranch} muted text={t("projectOverview.git.reading")} />;
	if (found.isError || !found.data)
		return <Line icon={GitBranch} muted text={t("projectOverview.git.none")} />;
	if (found.data.inside) return <RepositoryStatus projectId={projectId} />;
	const repositories = found.data.repositories;
	if (repositories.length === 0)
		return <Line icon={GitBranch} muted text={t("projectOverview.git.none")} />;
	return (
		<ul className="space-y-1">
			{repositories.slice(0, MOST_REPOSITORIES).map((repository) => (
				<li key={repository.path}>
					<Line
						icon={GitBranch}
						text={
							<>
								<span className="text-heading font-mono">{repository.path}</span>
								{repository.branch ? (
									<span className="text-muted-foreground"> · {repository.branch}</span>
								) : null}
							</>
						}
					/>
				</li>
			))}
			{repositories.length > MOST_REPOSITORIES ? (
				<li className="text-muted-foreground pl-6 text-xs">
					{t("projectOverview.git.more", { count: repositories.length - MOST_REPOSITORIES })}
				</li>
			) : null}
		</ul>
	);
}

/** The branch, how far it is from its upstream, and what changed. */
function RepositoryStatus({ projectId }: { projectId: string }) {
	const { t } = useTranslation("projects");
	const status = useGitStatus(projectId, undefined, true, GLANCE);
	if (!status.data) return <Line icon={GitBranch} muted text={t("projectOverview.git.reading")} />;
	const { branch, head, ahead, behind, changes, conflicts, operation } = status.data;
	return (
		<div className="space-y-1">
			<Line
				icon={GitBranch}
				text={
					<span className="flex flex-wrap items-center gap-x-2">
						<span className="text-heading font-mono">
							{branch ?? t("projectOverview.git.detached", { head: head ?? "" })}
						</span>
						{ahead ? (
							<span className="text-muted-foreground flex items-center tabular-nums">
								<ArrowUp className="size-3" aria-label={t("projectOverview.git.ahead")} />
								{ahead}
							</span>
						) : null}
						{behind ? (
							<span className="text-muted-foreground flex items-center tabular-nums">
								<ArrowDown className="size-3" aria-label={t("projectOverview.git.behind")} />
								{behind}
							</span>
						) : null}
					</span>
				}
			/>
			<Line
				icon={GitCommitVertical}
				muted={changes.length === 0}
				text={
					changes.length
						? t("projectOverview.git.changes", { count: changes.length })
						: t("projectOverview.git.clean")
				}
			/>
			{conflicts.length || operation ? (
				<Line
					icon={CircleAlert}
					warning
					text={
						conflicts.length
							? t("projectOverview.git.conflicts", { count: conflicts.length })
							: t("projectOverview.git.operation", { operation })
					}
				/>
			) : null}
		</div>
	);
}

function Line({
	icon: Icon,
	text,
	muted,
	warning,
}: {
	icon: LucideIcon;
	text: ReactNode;
	muted?: boolean;
	warning?: boolean;
}) {
	return (
		<p
			className={cn(
				"flex min-w-0 items-center gap-2 text-xs",
				muted && "text-muted-foreground",
				warning && "text-warning",
			)}
		>
			<Icon className="size-3.5 shrink-0" aria-hidden />
			<span className="min-w-0 truncate">{text}</span>
		</p>
	);
}
