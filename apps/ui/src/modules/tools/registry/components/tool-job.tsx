import { cn } from "cn";
import { LoaderCircle } from "lucide-react";

import { jobLine } from "@/modules/tools/registry/model";
import type { ToolJobDto } from "@/shared/api/generated/ahab.schemas";

/** How many of a job's last output lines show. */
const LINES = 6;

/** An install, update or removal: its step, its last output lines, or where it failed. */
export function ToolJob({ job, className }: { job: ToolJobDto; className?: string }) {
	const failed = job.state === "failed";
	const output = (job.output ?? []).slice(-LINES);
	return (
		<div
			role={failed ? "alert" : "status"}
			className={cn(
				"space-y-1.5 rounded-md border px-3 py-2 text-xs",
				failed ? "border-destructive/40 bg-destructive/6" : "border-border bg-muted/40",
				className,
			)}
		>
			<p className={cn("flex items-center gap-1.5", failed && "text-destructive")}>
				{job.state === "running" && <LoaderCircle className="size-3.5 animate-spin" />}
				{jobLine(job)}
			</p>
			{output.length > 0 && (
				<pre className="text-muted-foreground max-h-32 overflow-auto font-mono text-[0.6875rem] leading-snug whitespace-pre-wrap">
					{output.join("\n")}
				</pre>
			)}
		</div>
	);
}
