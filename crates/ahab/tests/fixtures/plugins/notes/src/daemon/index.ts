import { definePlugin } from "@ahab/plugin/node";

import { forget, report } from "../shared/index.ts";

export default definePlugin((ahab) => {
	const notes = ahab.diagnostics.collection("notes");
	ahab.commands.handle(report, (_, call) => {
		if (!call.project) throw new Error("report needs a project");
		notes.set(call.project, {
			"README.md": [
				{
					range: { start: { line: 0, character: 2 }, end: { line: 0, character: 7 } },
					severity: "warning",
					message: "A note about the title",
					code: "title",
					fixes: [
						{
							title: "Call it “Heading”",
							edits: [
								{
									range: { start: { line: 0, character: 2 }, end: { line: 0, character: 7 } },
									newText: "Heading",
								},
							],
						},
					],
				},
			],
			// Outside the project: dropped.
			"../escape.md": [{ range: { start: { line: 0 } }, message: "escaped" }],
			"/etc/passwd": [{ range: { start: { line: 0 } }, message: "escaped" }],
		});
		return { reported: true };
	});
	ahab.commands.handle(forget, (_, call) => {
		if (!call.project) throw new Error("forget needs a project");
		notes.delete(call.project, "README.md");
		return { forgot: true };
	});
	ahab.diagnostics.onCheck(({ project, roots, reason }) => {
		for (const root of roots) {
			notes.set(project, root ? `${root}/notes.txt` : "notes.txt", [
				{
					range: { start: { line: 0 } },
					severity: "information",
					message: `checked for ${reason}`,
				},
			]);
		}
	});
});
