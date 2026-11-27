import { ListTodo } from "lucide-react";

import { type Entry, itemText } from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";

export interface Step {
	text: string;
	state: "pending" | "active" | "complete";
}

export function stepsOf(entry: Entry): Step[] | undefined {
	if (entry.kind !== "item") return;
	const item = entry.item;
	if (item.type === "plan") {
		const lines = itemText(entry).trim().split("\n");
		const steps: Step[] = [];
		for (const line of lines) {
			if (!line.trim()) continue;
			const match = /^\s*[-*]\s+\[([ xX~-])\]\s+(.+)/.exec(line);
			if (!match?.[2]) return;
			steps.push({
				text: match[2],
				state: match[1] === " " ? "pending" : /x/i.test(match[1] ?? "") ? "complete" : "active",
			});
		}
		return steps;
	}
	if (
		item.type !== "toolCall" ||
		item.detail.kind !== "other" ||
		item.detail.name !== "TodoWrite" ||
		item.status === "failed" ||
		item.status === "declined"
	)
		return;
	const todos = (item.detail.input as { todos?: unknown } | null)?.todos;
	if (!Array.isArray(todos)) return;
	return todos.flatMap((row): Step[] =>
		typeof row?.content === "string"
			? [
					{
						text: row.content,
						state:
							row.status === "completed"
								? "complete"
								: row.status === "in_progress"
									? "active"
									: "pending",
					},
				]
			: [],
	);
}

export function PlanProgress({ steps, pinned = false }: { steps: Step[]; pinned?: boolean }) {
	const { t } = useTranslation("sessions");
	const done = steps.filter((step) => step.state === "complete").length;
	return (
		<details
			open={pinned ? undefined : true}
			className="text-muted-foreground min-w-0 py-1 text-xs"
		>
			<summary className="cursor-pointer">
				<ListTodo className="mr-1 inline size-4" />
				{t("conversation.plan")} · {done}/{steps.length}
				{pinned && (
					<span className="ml-2">{steps.find((step) => step.state === "active")?.text}</span>
				)}
			</summary>
			<ul className="max-h-40 space-y-1 overflow-auto py-2 pl-5">
				{steps.map((step, i) => (
					// oxlint-disable-next-line react/no-array-index-key -- positions belong to this ordered snapshot.
					<li key={i} className={step.state === "complete" ? "line-through" : "text-foreground"}>
						<span aria-hidden>
							{step.state === "complete" ? "✓ " : step.state === "active" ? "◐ " : "○ "}
						</span>
						<span className="sr-only">{t(`tools.step.${step.state}`)} </span>
						{step.text}
					</li>
				))}
			</ul>
		</details>
	);
}
