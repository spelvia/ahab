import { useMemo } from "react";

import { chatSettings, editorBehaviour, editorSettings } from "@/modules/platform/setting/model";
import {
	useChangeSettings as useGeneratedChangeSettings,
	useGetSettings,
} from "@/shared/api/generated/platform-setting/platform-setting";
import { useInvalidate } from "@/shared/api/invalidate";

/** A section of Ahab's settings, for a project when given. */
export function useSettings(section: string, project?: string) {
	return useGetSettings(section, project ? { project } : undefined, {
		query: { staleTime: 30_000, retry: false },
	});
}

/** The editor's settings in effect, for a project when given; defaults while they load. */
export function useEditorSettings(project?: string) {
	const settings = useSettings("editor", project);
	const values = settings.data?.values as Record<string, unknown> | undefined;
	return useMemo(
		() => ({
			look: editorSettings(values),
			behaviour: editorBehaviour(values),
			loaded: Boolean(values),
		}),
		[values],
	);
}

/** The chat composer's settings in effect; defaults while they load. */
export function useChatSettings() {
	const settings = useSettings("editor");
	const values = settings.data?.values as Record<string, unknown> | undefined;
	return useMemo(() => chatSettings(values), [values]);
}

/** Changes settings for the server, or a project (`params.project`). */
export function useChangeSettings() {
	return useGeneratedChangeSettings({ mutation: { onSuccess: useInvalidate("/settings/") } });
}
