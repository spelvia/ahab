import { describe, expect, it } from "vitest";

import { chatLocations, isFileLocation } from "@/shared/ui/markdown-links";

describe("isFileLocation", () => {
	it("takes paths and known file names for files", () => {
		expect(isFileLocation("src/main.rs:12")).toBe(true);
		expect(isFileLocation("./a.b:3:4")).toBe(true);
		expect(isFileLocation("main.rs:12")).toBe(true);
		expect(isFileLocation("README.md:1")).toBe(true);
	});

	it("leaves hosts and ports alone", () => {
		expect(isFileLocation("api.example.com:443")).toBe(false);
		expect(isFileLocation("localhost.localdomain:8080")).toBe(false);
		expect(isFileLocation("db.internal:5432")).toBe(false);
	});
});

describe("chatLocations", () => {
	it("links file locations in prose but not hosts", () => {
		const tree = {
			type: "root",
			children: [
				{
					type: "element",
					tagName: "p",
					children: [{ type: "text", value: "See main.rs:4 and call api.example.com:443 now" }],
				},
			],
		};
		chatLocations()(tree);
		const links = tree.children[0]?.children.filter(
			(node) => (node as { tagName?: string }).tagName === "a",
		);
		expect(
			links?.map((node) => (node as { properties?: { href: string } }).properties?.href),
		).toEqual(["main.rs:4"]);
	});
});
