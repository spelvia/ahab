import type { Problem } from "@/shared/api/generated/ahab.schemas";
import { markNotPaired } from "@/shared/api/pairing";
import { t } from "@/shared/i18n";

/** An error response from the API, thrown so TanStack Query sees a failure. */
export class ApiError<Body = Problem> extends Error {
	readonly status: number;
	/** The Problem Details body, when the server sent one. */
	readonly body: Body | undefined;

	constructor(status: number, body: Body | undefined, message?: string) {
		const problem = body as Partial<Problem> | undefined;
		super(message ?? problem?.detail ?? problem?.title ?? t("errors.status", { status }));
		this.name = "ApiError";
		this.status = status;
		this.body = body;
	}
}

/** The stable `code` of a Problem Details error from the API, when there is one. */
export function problemCode(error: unknown) {
	return error instanceof ApiError ? error.body?.code : undefined;
}

/** The error type of every generated query and mutation. */
export type ErrorType<Body> = ApiError<Body>;

/**
 * Calls the API on the same origin. The generated client sends every request
 * through here.
 */
export async function apiFetch<T>(
	url: string,
	{ timeoutMs, ...init }: RequestInit & { timeoutMs?: number },
): Promise<T> {
	const deadline =
		timeoutMs ?? (init.method === "GET" ? (url.includes("/git/") ? 90_000 : 45_000) : undefined);
	const timeout = deadline === undefined ? undefined : AbortSignal.timeout(deadline);
	const signal =
		timeout && init.signal ? AbortSignal.any([init.signal, timeout]) : (timeout ?? init.signal);
	let response: Response;
	let text: string;
	// Look up fetch on every call, so the mock API in tests is used.
	try {
		response = await globalThis.fetch(new URL(url, globalThis.location?.origin), {
			...init,
			signal,
		});
		text = await response.text();
	} catch (error) {
		if (init.signal?.aborted) throw error;
		throw new ApiError(
			0,
			undefined,
			t(timeout?.aborted ? "errors.requestTimeout" : "errors.serverUnreachable"),
		);
	}
	const type = response.headers.get("content-type") ?? "";
	const isJson = type.includes("json");
	// Text answers, such as a reading's HTML or source, are their text.
	let body: unknown;
	try {
		body = isJson && text ? JSON.parse(text) : type.startsWith("text/") ? text : undefined;
	} catch {
		if (response.ok) throw new ApiError(response.status, undefined, t("errors.invalidResponse"));
	}
	if (!response.ok) {
		const problem = body as Problem | undefined;
		// This browser is not paired (any more): the app shows the pairing screen.
		if (response.status === 401 && problem?.code === "not_paired") markNotPaired();
		throw new ApiError(response.status, problem);
	}
	return body as T;
}
