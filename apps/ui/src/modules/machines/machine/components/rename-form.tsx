import { type FormEvent, useState } from "react";

import { useRenameMachine } from "@/modules/machines/machine/api";
import { nameProblem } from "@/modules/machines/machine/model";
import type { Machine } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/** Renames a machine in place: the name becomes a field. Escape cancels. */
export function RenameForm({ machine, onDone }: { machine: Machine; onDone: () => void }) {
	const { t } = useTranslation("machines");
	const { t: common } = useTranslation();
	const [name, setName] = useState(machine.name);
	const rename = useRenameMachine();
	const problem = nameProblem(name);
	const hint = problem ?? rename.error?.message;

	function submit(event: FormEvent) {
		event.preventDefault();
		if (problem) return;
		if (name === machine.name) return onDone();
		rename.mutate({ id: machine.id, data: { name } }, { onSuccess: onDone });
	}

	return (
		<form onSubmit={submit} className="flex flex-col gap-1">
			<div className="flex flex-wrap items-center gap-2">
				<label htmlFor={`name-${machine.id}`} className="sr-only">
					{t("renameForm.name")}
				</label>
				<input
					id={`name-${machine.id}`}
					value={name}
					onChange={(event) => setName(event.target.value)}
					onKeyDown={(event) => event.key === "Escape" && onDone()}
					aria-invalid={hint !== undefined}
					aria-describedby={`name-${machine.id}-hint`}
					autoFocus
					spellCheck={false}
					className="border-input bg-background focus-visible:ring-ring/40 aria-invalid:border-destructive h-8 w-56 rounded-md border px-2 font-mono text-sm outline-none focus-visible:ring-[3px]"
				/>
				<Button type="submit" size="sm" disabled={problem !== undefined || rename.isPending}>
					{common("actions.save")}
				</Button>
				<Button type="button" variant="ghost" size="sm" onClick={onDone}>
					{common("actions.cancel")}
				</Button>
			</div>
			<p id={`name-${machine.id}-hint`} className="text-destructive min-h-4 text-xs" role="alert">
				{hint}
			</p>
		</form>
	);
}
