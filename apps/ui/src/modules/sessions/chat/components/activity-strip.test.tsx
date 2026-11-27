import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";

import { SubagentStrip, TaskStrip } from "@/modules/sessions/chat/components/activity-strip";
import type { ObservedAgent, ObservedTask } from "@/modules/sessions/chat/model";

const agent: ObservedAgent = {
	id: "a",
	parent: { type: "root" },
	name: "Explorer",
	status: "running",
};
const task: ObservedTask = {
	id: "t",
	kind: { type: "command", command: "npm test" },
	status: "running",
};

it("names what each Stop stops, and shows each item's own stopping state", async () => {
	const stop = vi.fn<(id: string, name: string) => void>();
	render(
		<>
			<SubagentStrip agents={[agent]} onOpen={vi.fn<() => void>()} onStop={stop} stops={{}} />
			<TaskStrip
				tasks={[task]}
				onOpen={vi.fn<() => void>()}
				onStop={stop}
				stops={{ t: { phase: "stopping" } }}
			/>
		</>,
	);
	await userEvent.click(screen.getByRole("button", { name: "Stop subagent Explorer" }));
	expect(stop).toHaveBeenCalledWith("a", "Explorer");
	const busy = screen.getByRole("button", { name: "Stop task npm test: stopping…" });
	expect(busy).toBeDisabled();
	expect(busy).toHaveTextContent("Stopping…");
});

it("shows a refused stop as an error, and lets it be tried again", () => {
	render(
		<SubagentStrip
			agents={[agent]}
			onOpen={vi.fn<() => void>()}
			onStop={vi.fn<() => void>()}
			stops={{ a: { phase: "failed", message: "It is busy" } }}
		/>,
	);
	expect(screen.getByRole("alert")).toHaveTextContent(
		"The agent would not stop Explorer: It is busy",
	);
	expect(screen.getByRole("button", { name: "Stop subagent Explorer" })).toBeEnabled();
});

it("says when the agent has not reported the stop in time", () => {
	render(
		<TaskStrip
			tasks={[task]}
			onOpen={vi.fn<() => void>()}
			onStop={vi.fn<() => void>()}
			stops={{ t: { phase: "timedOut" } }}
		/>,
	);
	expect(screen.getByRole("status")).toHaveTextContent("npm test still runs");
});

it("lists a task by what it runs, without a live line of its output", () => {
	render(
		<TaskStrip
			tasks={[
				{
					...task,
					description: "Run the tests",
					progress: { summary: "PASS src/a.test.ts", lastToolName: "Bash" },
				} as ObservedTask,
			]}
			onOpen={vi.fn<() => void>()}
		/>,
	);
	expect(screen.getByText("npm test")).toBeInTheDocument();
	expect(screen.getByText("Run the tests")).toBeInTheDocument();
	expect(screen.queryByText("PASS src/a.test.ts")).not.toBeInTheDocument();
	expect(screen.queryByText("Bash")).not.toBeInTheDocument();
});
