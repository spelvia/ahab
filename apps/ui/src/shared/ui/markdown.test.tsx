import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { Markdown } from "@/shared/ui/markdown";

describe("Markdown", () => {
	it("typesets math once KaTeX has loaded", async () => {
		const { container } = render(
			<Markdown text={"Euler: \\(e^{i\\pi} + 1 = 0\\)\n\n$$\nx^2\n$$"} />,
		);
		await screen.findAllByText((_, element) => element?.classList.contains("katex") ?? false);
		expect(container.querySelectorAll(".katex")).toHaveLength(2);
		expect(container.querySelector(".katex-display")).not.toBeNull();
	});

	it("opens local file links at their requested line and keeps web links external", () => {
		const open = vi.fn<(path: string, line?: number) => void>();
		render(
			<Markdown
				files={{ root: "/work/repo", open }}
				text={
					"[relative](./src/a%20b.ts#L8-L12) [absolute](/work/repo/main.rs:4:2) [URI](file:///work/repo/README.md) [bare](README.md:7) [web](https://example.org) [outside](/etc/hosts)"
				}
			/>,
		);
		for (const name of ["relative", "absolute", "URI", "bare"])
			fireEvent.click(screen.getByRole("link", { name }));
		expect(open.mock.calls).toEqual([
			["src/a b.ts", 8],
			["main.rs", 4],
			["README.md", undefined],
			["README.md", 7],
		]);
		expect(screen.getByRole("link", { name: "web" })).toHaveAttribute("target", "_blank");
		expect(screen.queryByRole("link", { name: "outside" })).not.toBeInTheDocument();
		expect(screen.getByText("outside")).toHaveTextContent("Link unavailable in this project");
	});

	it("loads a README's HTML logo and Markdown images relative to the document", () => {
		const { container } = render(
			<Markdown
				imageBase={{ project: "work", path: "docs/README.md" }}
				text={
					'<img src="../icon.svg" width="80" height="80" alt="Logo" onerror="alert(1)">\n\n![Chart](chart%201.png)'
				}
			/>,
		);
		const logo = screen.getByAltText("Logo");
		expect(logo).toHaveAttribute("src", "/api/v1/projects/work/image?path=icon.svg");
		expect(logo).toHaveAttribute("width", "80");
		expect(logo).not.toHaveAttribute("onerror");
		expect(container.querySelectorAll("img")).toHaveLength(2);
		expect(screen.getByAltText("Chart").getAttribute("src")).toContain("path=docs%2Fchart+1.png");
	});

	it("shows a document's HTML as GitHub does, sanitized", () => {
		const { container } = render(
			<Markdown
				html
				imageBase={{ project: "work", path: "README.md" }}
				text={
					'<p align="center">\n  <img src="apps/ui/public/favicon.svg" width="80" height="80" alt="Logo">\n</p>\n<h1 align="center">Ahab</h1>\n<p align="center">A project.<br> <a href="https://x.org" onclick="alert(1)">Site</a><script>alert(1)</script></p>\n\n## Start\n\n<details><summary>More</summary>Hidden</details>'
				}
			/>,
		);
		const logo = screen.getByAltText("Logo");
		expect(logo).toHaveAttribute(
			"src",
			"/api/v1/projects/work/image?path=apps%2Fui%2Fpublic%2Ffavicon.svg",
		);
		expect(logo).toHaveAttribute("width", "80");
		expect(logo.parentElement).toHaveAttribute("align", "center");
		expect(screen.getByRole("heading", { level: 1, name: "Ahab" })).toBeInTheDocument();
		expect(screen.getByRole("heading", { level: 2, name: "Start" })).toHaveAttribute("id", "start");
		expect(screen.getByRole("link", { name: "Site" })).not.toHaveAttribute("onclick");
		expect(container.querySelector("script")).toBeNull();
		expect(container.querySelector("details summary")?.textContent).toBe("More");
	});

	it("does not render a chat's HTML", () => {
		const { container } = render(<Markdown text={'Use a <p align="center"> here.'} />);
		expect(container.querySelector("p[align]")).toBeNull();
	});

	it("opens an image in a viewer that zooms, and leaves a linked image to its link", () => {
		render(
			<Markdown
				text={
					"![Chart](https://example.org/a.png) [![Badge](https://example.org/b.png)](https://example.org)"
				}
			/>,
		);
		expect(screen.getByAltText("Badge")).not.toHaveAttribute("role");
		fireEvent.click(screen.getByRole("button", { name: "Open the image: Chart" }));
		const viewer = screen.getByRole("dialog", { name: "Chart" });
		expect(viewer).toBeInTheDocument();
		const zoom = screen.getByTitle("Switch between fitted and actual size");
		expect(zoom).toHaveTextContent("100%");
		fireEvent.click(screen.getByRole("button", { name: "Zoom in (+)" }));
		expect(zoom).toHaveTextContent("125%");
		fireEvent.keyDown(viewer, { key: "-" });
		fireEvent.keyDown(viewer, { key: "-" });
		expect(zoom).toHaveTextContent("80%");
		fireEvent.keyDown(viewer, { key: "0" });
		expect(zoom).toHaveTextContent("100%");
		fireEvent.click(screen.getByRole("button", { name: "Close" }));
		expect(screen.queryByRole("dialog")).toBeNull();
	});

	it("leaves text without math as it is", () => {
		const { container } = render(<Markdown text="Costs **5** euros." />);
		expect(container.querySelector(".katex")).toBeNull();
		expect(screen.getByText("5").tagName).toBe("STRONG");
	});
});
