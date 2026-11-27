import { useQueryClient } from "@tanstack/react-query";

/**
 * A function that asks again for every query whose address contains one of
 * `parts` (`"/sessions"`, `"/projects/<id>/git/"`), and only those: a
 * change refreshes what it can affect, not every query on the page. Layouts
 * are never refreshed this way: only the tiles change them, and refetching
 * one would reset the tiles while they are open.
 */
export function useInvalidate(...parts: string[]) {
	const queryClient = useQueryClient();
	return () =>
		void queryClient.invalidateQueries({
			predicate: (query) => {
				const key = String(query.queryKey[0]);
				return !key.includes("/layouts/") && parts.some((part) => key.includes(part));
			},
		});
}
