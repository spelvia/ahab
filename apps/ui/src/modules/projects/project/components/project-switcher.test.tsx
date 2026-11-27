import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";

import { ProjectSwitcher } from "@/modules/projects/project/components/project-switcher";
import type { Project } from "@/shared/api/generated/ahab.schemas";

const project = (id: string, name: string) =>
	({
		id,
		name,
		group: null,
		machineName: "atlas",
		path: `/work/${name}`,
		icon: null,
	}) as unknown as Project;

/** The workbench's: a key for places 0 to 8, and a failure for any other. */
const shortcut = (index: number) => {
	if (index < 0 || index > 8) throw new Error(`no project.${index + 1}`);
	return `Alt+${index + 1}`;
};

const switcher = (projects: Project[]) =>
	render(
		<QueryClientProvider client={new QueryClient()}>
			<ProjectSwitcher
				projects={projects}
				onSwitch={() => {}}
				onOpenFolder={() => {}}
				onManage={() => {}}
				shortcut={shortcut}
			/>
		</QueryClientProvider>,
	);

it("opens with no projects yet, as while they load", async () => {
	switcher([]);
	await userEvent.click(screen.getByRole("button", { name: "Switch project" }));
	expect(screen.getByRole("menuitem", { name: /Open folder/ })).toBeInTheDocument();
});

it("shows each project's switch key and the range in its hint", async () => {
	switcher([project("1", "ahab"), project("2", "stubb")]);
	await userEvent.click(screen.getByRole("button", { name: "Switch project" }));
	expect(screen.getByText("Alt+2")).toBeInTheDocument();
	expect(screen.getByText(/Alt\+1…Alt\+2 to switch/)).toBeInTheDocument();
});
