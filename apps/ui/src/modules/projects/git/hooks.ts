import { useState } from "react";

import { GLANCE, useGitStatus, useRepositories } from "@/modules/projects/git/api";
import type { Project } from "@/shared/api/generated/ahab.schemas";
import { gitRepository, rememberGitRepository } from "@/shared/preferences/store";

/**
 * Where a project's Git is: its folder's repository, or the one chosen
 * among those inside it (remembered per project); with its status at a
 * glance (the status bar's branch, the count on the Git view's icon).
 */
export function useProjectRepository(project: Project | undefined) {
	const reachable = project?.availability === "available";
	const repos = useRepositories(project?.id, reachable);
	const [chosen, setChosen] = useState<Record<string, string>>({});
	const repositories = repos.data?.repositories ?? [];
	const wanted = project ? (chosen[project.id] ?? gitRepository(project.id)) : undefined;
	const repository = repos.data?.inside
		? undefined
		: (repositories.find((item) => item.path === wanted) ?? repositories[0])?.path;
	const ready = Boolean(repos.data);
	const choose = (path: string) => {
		if (!project) return;
		rememberGitRepository(project.id, path);
		setChosen((current) => ({ ...current, [project.id]: path }));
	};
	const status = useGitStatus(project?.id, repository, reachable && ready, GLANCE);
	return { repository, repositories, ready, choose, status };
}
