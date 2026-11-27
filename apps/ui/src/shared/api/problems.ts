import { problemCode } from "@/shared/api/fetch";
import { t } from "@/shared/i18n";

/** What the codes every machine request may answer mean, in a sentence (their keys). */
const COMMON = {
	machine_offline: "errors.machineOffline",
	folder_missing: "errors.folderMissing",
	permission_denied: "errors.permissionDenied",
	project_unknown: "errors.projectUnknown",
	path_outside_folder: "errors.outsideFolder",
} as const;

/**
 * An error as a sentence: the view's own words for its codes first, then
 * the common ones, then the server's message, else `fallback`.
 */
export function explain(
	error: unknown,
	fallback: string,
	own: Record<string, string> = {},
): string {
	const code = problemCode(error);
	if (code && own[code]) return own[code];
	if (code && code in COMMON) return t(COMMON[code as keyof typeof COMMON]);
	return error instanceof Error && error.message ? error.message : fallback;
}
