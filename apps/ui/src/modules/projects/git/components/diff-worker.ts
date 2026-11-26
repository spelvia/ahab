import { type Row, sideBySide } from "@/modules/projects/git/model";

const scope = self as unknown as {
	addEventListener(
		type: "message",
		listener: (event: MessageEvent<{ before: string; after: string }>) => void,
	): void;
	postMessage(rows: Row[], transfer: Transferable[]): void;
};

scope.addEventListener("message", ({ data }) => {
	scope.postMessage(sideBySide(data.before, data.after), []);
});
