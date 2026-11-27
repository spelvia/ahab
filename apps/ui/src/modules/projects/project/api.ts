import {
	getProject,
	useCreateProject as useGeneratedCreateProject,
	useGetProject,
	useListProjects,
	useOpenFolder as useGeneratedOpenFolder,
	useUpdateProject as useGeneratedUpdateProject,
	useDeleteProject as useGeneratedDeleteProject,
	useRemoveProjectIcon as useGeneratedRemoveProjectIcon,
	useSetProjectIcon as useGeneratedSetProjectIcon,
	useSetProjectOrder as useGeneratedSetProjectOrder,
} from "@/shared/api/generated/projects-project/projects-project";
import { useInvalidate } from "@/shared/api/invalidate";

/** Active projects in their order; asked again now and then, for their state. */
export function useProjects() {
	return useListProjects(undefined, { query: { refetchInterval: 10_000 } });
}

/** Archived projects. */
export function useArchivedProjects() {
	return useListProjects({ state: "archived" });
}

/** One project by name or ID, with its state. */
export function useProject(project: string) {
	return useGetProject(project, { query: { refetchInterval: 5_000, retry: false } });
}

/** A project, asked once (its machine, for language tools). */
export function fetchProject(id: string) {
	return getProject(id);
}

/** Every project query: the lists and each project's own. */
function useRefreshProjects() {
	return useInvalidate("/projects");
}

/** Creates a project for a folder on a machine. */
export function useCreateProject() {
	const refresh = useRefreshProjects();
	return useGeneratedCreateProject({ mutation: { onSuccess: refresh } });
}

/** Opens a folder: its project, or a new one named after it. */
export function useOpenFolder() {
	const refresh = useRefreshProjects();
	return useGeneratedOpenFolder({ mutation: { onSuccess: refresh } });
}

/** Renames, groups, archives or restores a project, or changes its mode. */
export function useUpdateProject() {
	const refresh = useRefreshProjects();
	return useGeneratedUpdateProject({ mutation: { onSuccess: refresh } });
}

/** Deletes a project; its files are untouched. */
export function useDeleteProject() {
	const refresh = useRefreshProjects();
	return useGeneratedDeleteProject({ mutation: { onSuccess: refresh } });
}

/** Orders the projects by hand. */
export function useSetProjectOrder() {
	const refresh = useRefreshProjects();
	return useGeneratedSetProjectOrder({ mutation: { onSuccess: refresh } });
}

/** Gives a project an emoji or an image instead of its initial. */
export function useSetProjectIcon() {
	const refresh = useRefreshProjects();
	return useGeneratedSetProjectIcon({ mutation: { onSuccess: refresh } });
}

/** Gives a project its initial back. */
export function useRemoveProjectIcon() {
	const refresh = useRefreshProjects();
	return useGeneratedRemoveProjectIcon({ mutation: { onSuccess: refresh } });
}
