/**
 * Ahab's bundled languages: the declarations in
 * `apps/ui/languages/languages.json`, their grammars and queries as files of
 * their own (content-hashed, cached for good), each fetched the first time
 * a language is needed. Rebuilt with `make grammars-update`.
 */

import { type LanguageDeclaration, type Language, resolveLanguage } from "@/shared/syntax/model";

import declarations from "../../../languages/languages.json";

const PREFIX = "../../../languages/";

/** Each grammar's and query's address, by its path in `apps/ui/languages/`. */
const files = import.meta.glob<string>(
	["../../../languages/grammars/*.wasm.gz", "../../../languages/queries/**/*.scm"],
	{
		query: "?url",
		import: "default",
		eager: true,
	},
);

export const bundledLanguages: Language[] = (declarations as LanguageDeclaration[]).map(
	(declaration) =>
		resolveLanguage(declaration, "", "bundled", (path) => files[PREFIX + path] ?? ""),
);
