import { Plus, X } from "lucide-react";

import { useTranslation } from "@/shared/i18n";
import {
	setVimMappings,
	useVimMappings,
	VIM_MAPPING_MODES,
	type VimMapping,
	type VimMappingProblem,
	vimMappingProblem,
} from "@/shared/keymap/vim";
import { Button } from "@/shared/ui/button";
import { Switch } from "@/shared/ui/switch";

const field =
	"border-input bg-background focus-visible:ring-ring h-8 rounded-md border px-2 text-sm outline-none focus-visible:ring-2";

/**
 * Settings → Keyboard's Vim mappings: keys typed and the keys they stand
 * for, per mode, as Vim's `:map` makes them; kept in this browser and
 * applied to every editor at once. A mapping that cannot be applied says
 * why and is left out.
 */
export function VimMappings() {
	const { t } = useTranslation("platform");
	const mappings = useVimMappings();
	const change = (index: number, next: Partial<VimMapping>) =>
		setVimMappings(
			mappings.map((mapping, at) => (at === index ? { ...mapping, ...next } : mapping)),
		);

	return (
		<section aria-labelledby="keys-vim" className="space-y-2 pt-2">
			<h2 id="keys-vim" className="text-heading text-base font-bold">
				{t("keyboard.vim.title")}
			</h2>
			<p className="text-muted-foreground max-w-2xl text-sm text-pretty">
				{t("keyboard.vim.intro")}
			</p>
			{mappings.length ? (
				<ul
					aria-label={t("keyboard.vim.title")}
					className="border-border divide-border divide-y border-y"
				>
					{mappings.map((mapping, index) => {
						const problem =
							mapping.lhs || mapping.rhs
								? vimMappingProblem(mapping, mappings.slice(0, index))
								: undefined;
						const number = index + 1;
						return (
							// A mapping is its place in the list: it has no other name.
							// oxlint-disable-next-line react/no-array-index-key
							<li key={index} className="space-y-1 px-3 py-3">
								<div className="flex flex-wrap items-center gap-2">
									<select
										aria-label={t("keyboard.vim.mode", { number })}
										value={mapping.mode}
										onChange={(event) =>
											change(index, { mode: event.target.value as VimMapping["mode"] })
										}
										className={field}
									>
										{VIM_MAPPING_MODES.map((mode) => (
											<option key={mode} value={mode}>
												{t(`keyboard.vim.modes.${mode}`)}
											</option>
										))}
									</select>
									<input
										aria-label={t("keyboard.vim.lhs", { number })}
										placeholder={t("keyboard.vim.lhsPlaceholder")}
										value={mapping.lhs}
										spellCheck={false}
										autoCapitalize="off"
										autoComplete="off"
										onChange={(event) => change(index, { lhs: event.target.value })}
										aria-invalid={problem ? true : undefined}
										className={`${field} w-32 font-mono`}
									/>
									<span aria-hidden className="text-muted-foreground text-sm">
										→
									</span>
									<input
										aria-label={t("keyboard.vim.rhs", { number })}
										placeholder={t("keyboard.vim.rhsPlaceholder")}
										value={mapping.rhs}
										spellCheck={false}
										autoCapitalize="off"
										autoComplete="off"
										onChange={(event) => change(index, { rhs: event.target.value })}
										aria-invalid={problem ? true : undefined}
										className={`${field} w-40 font-mono`}
									/>
									<label className="text-muted-foreground flex items-center gap-2 text-sm">
										<Switch
											checked={Boolean(mapping.recursive)}
											onCheckedChange={(recursive) => change(index, { recursive })}
											label={t("keyboard.vim.recursive", { number })}
										/>
										{t("keyboard.vim.recursiveShort")}
									</label>
									<Button
										size="xs"
										variant="ghost"
										className="ml-auto"
										aria-label={t("keyboard.vim.remove", { number })}
										onClick={() => setVimMappings(mappings.filter((_, at) => at !== index))}
									>
										<X />
									</Button>
								</div>
								{problem ? (
									<p role="alert" className="text-destructive text-xs">
										<ProblemText problem={problem} />
									</p>
								) : null}
							</li>
						);
					})}
				</ul>
			) : (
				<p className="text-muted-foreground text-sm">{t("keyboard.vim.none")}</p>
			)}
			<Button
				size="sm"
				variant="outline"
				onClick={() => setVimMappings([...mappings, { mode: "normal", lhs: "", rhs: "" }])}
			>
				<Plus />
				{t("keyboard.vim.add")}
			</Button>
			<p className="text-muted-foreground max-w-2xl text-xs text-pretty">
				{t("keyboard.vim.ahabKeys")}
			</p>
		</section>
	);
}

function ProblemText({ problem }: { problem: VimMappingProblem }) {
	const { t } = useTranslation("platform");
	if (problem.problem === "unknownKey")
		return <>{t("keyboard.vim.problems.unknownKey", { key: problem.key })}</>;
	return <>{t(`keyboard.vim.problems.${problem.problem}`)}</>;
}
