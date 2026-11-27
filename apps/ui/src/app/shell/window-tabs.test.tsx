import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { type ComponentProps, createRef } from "react";
import { describe, expect, it, vi } from "vitest";

import { WindowTab } from "@/app/shell/window-tabs";
import { Workbench } from "@/app/shell/workbench-tiles";
import { Tiles, type TilesHandle, type OpenTab, type Tab } from "@/modules/projects";

type LayoutStore = ComponentProps<typeof Tiles>["store"];

const panels = { test: () => <input aria-label="Draft" /> };
const tab = (id: string, keep = false): Tab => ({
	id,
	component: "test",
	title: id,
	params: {},
	keep,
});

function setup(layout: LayoutStore["layout"] = null) {
	const ref = createRef<TilesHandle>();
	let tabs: OpenTab[] = [];
	const save = vi.fn<LayoutStore["save"]>().mockResolvedValue(1);
	const view = render(
		<Tiles
			ref={ref}
			store={{ key: "test", layout, failed: false, save }}
			empty={() => null}
			panels={panels}
			tabComponent={WindowTab}
			onTabs={(next) => {
				tabs = next;
			}}
		/>,
	);
	return { ref, view, save, ids: () => tabs.map((item) => item.id) };
}

describe("preview tabs", () => {
	it("replaces previews, keeps a double-clicked tab, and reuses existing tabs", async () => {
		const { ref, ids } = setup();
		act(() => ref.current?.open(tab("file")));
		act(() => ref.current?.open(tab("plugin")));
		expect(ids()).toEqual(["plugin"]);
		const title = await screen.findByText("plugin");
		expect(title.closest(".italic")).not.toBeNull();
		fireEvent.doubleClick(title);
		await waitFor(() => expect(title.closest(".italic")).toBeNull());
		act(() => ref.current?.open(tab("new-session")));
		expect(ids()).toEqual(["plugin", "new-session"]);
		act(() => ref.current?.open(tab("plugin")));
		expect(ids()).toEqual(["plugin", "new-session"]);
		act(() => ref.current?.open(tab("session")));
		expect(ids()).toEqual(["plugin", "session"]);
	});

	it("protects a draft when typing and keeps a started session", async () => {
		const { ref, ids } = setup();
		act(() => ref.current?.open(tab("new-session")));
		fireEvent.input(await screen.findByRole("textbox", { name: "Draft" }), {
			target: { value: "Keep my draft" },
		});
		act(() => ref.current?.open(tab("file")));
		expect(ids()).toEqual(["new-session", "file"]);
		act(() => ref.current?.open(tab("started-session", true), "new-session"));
		act(() => ref.current?.open(tab("plugin")));
		expect(ids()).toEqual(["started-session", "plugin"]);
	});

	it("reopens a kept tab without filling history with replaced previews", () => {
		const { ref, ids } = setup();
		act(() => ref.current?.open(tab("kept", true)));
		act(() => ref.current?.closeActive());
		act(() => ref.current?.open(tab("a")));
		act(() => ref.current?.open(tab("b")));
		act(() => ref.current?.reopenClosed());
		expect(ids()).toEqual(["b", "kept"]);
		act(() => ref.current?.open(tab("c")));
		expect(ids()).toEqual(["c", "kept"]);
	});

	it("saves and restores which tabs are previews", async () => {
		const first = setup();
		act(() => first.ref.current?.open(tab("kept", true)));
		act(() => first.ref.current?.open(tab("preview")));
		await waitFor(() => expect(first.save).toHaveBeenCalled(), { timeout: 1500 });
		const layout = first.save.mock.lastCall?.[0];
		first.view.unmount();
		const restored = setup({ layout, revision: 1 });
		act(() => restored.ref.current?.open(tab("replacement")));
		expect(restored.ids()).toEqual(["kept", "replacement"]);
	});

	it("keeps independent previews in split panes", () => {
		const { ref, ids } = setup();
		act(() => ref.current?.open(tab("left", true)));
		act(() => ref.current?.open(tab("right")));
		act(() => ref.current?.splitActive("right"));
		act(() => ref.current?.open(tab("right-preview")));
		act(() => ref.current?.focus("left"));
		act(() => ref.current?.open(tab("left-preview")));
		act(() => ref.current?.open(tab("next-left-preview")));
		expect(ids()).toEqual(["left", "next-left-preview", "right", "right-preview"]);
	});
});

const projectAt = (id: string, path: string, machineName: string) =>
	({ id, path, machineName, name: id, mode: "edit" }) as never;
const workbench = {
	project: projectAt("w1", "/mnt/data/ahab", "atlas"),
	onBrowse: () => {},
	onCommand: () => {},
	onSessionStarted: () => {},
	onNewSession: () => {},
	onOpenFile: () => {},
};
const file = (projectId: string, path: string): Tab => ({
	id: `${projectId}:${path}`,
	component: "file",
	title: path.split("/").pop() ?? path,
	params: { kind: "file", projectId, projectName: projectId, path },
	keep: true,
});

describe("tab tooltips", () => {
	it("shows a file's path on its machine", async () => {
		const ref = createRef<TilesHandle>();
		const save = vi.fn<LayoutStore["save"]>().mockResolvedValue(1);
		render(
			<Workbench.Provider value={workbench}>
				<Tiles
					ref={ref}
					store={{ key: "test", layout: null, failed: false, save }}
					empty={() => null}
					panels={{ file: () => null }}
					tabComponent={WindowTab}
				/>
			</Workbench.Provider>,
		);
		act(() => ref.current?.open(file("w1", "crates/ahab/src/main.rs")));
		const main = await screen.findByText("main.rs");
		expect(main.closest("[title]")?.getAttribute("title")).toBe(
			"/mnt/data/ahab/crates/ahab/src/main.rs — atlas",
		);
	});
});
