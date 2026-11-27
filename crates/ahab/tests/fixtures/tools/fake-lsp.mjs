// A tiny language server for Ahab's tests: it speaks LSP over
// stdio, reports a warning on every line with TODO and an error on every
// line with ERROR, answers hover, definition (a word `outside` leads out of
// the workspace, to FAKE_OUTSIDE), references, formatting (trailing spaces
// go) and workspace/configuration, and exits when asked.

const documents = new Map();
let buffer = Buffer.alloc(0);
let configuration = null;

function send(message) {
	const body = Buffer.from(JSON.stringify({ jsonrpc: "2.0", ...message }), "utf8");
	process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
	process.stdout.write(body);
}

function publish(uri) {
	const text = documents.get(uri) ?? "";
	const diagnostics = [];
	text.split("\n").forEach((line, index) => {
		for (const [word, severity] of [
			["TODO", 2],
			["ERROR", 1],
		]) {
			const at = line.indexOf(word);
			if (at !== -1) {
				diagnostics.push({
					range: {
						start: { line: index, character: at },
						end: { line: index, character: at + word.length },
					},
					severity,
					source: "fake",
					code: word.toLowerCase(),
					message: `${word} found`,
				});
			}
		}
	});
	send({ method: "textDocument/publishDiagnostics", params: { uri, diagnostics } });
}

function wordAt(uri, position) {
	const line = (documents.get(uri) ?? "").split("\n")[position.line] ?? "";
	const before = line.slice(0, position.character).match(/\w*$/)[0];
	const after = line.slice(position.character).match(/^\w*/)[0];
	return before + after;
}

function handle(message) {
	const { id, method, params } = message;
	if (method === undefined) {
		// An answer to our request.
		if (id === "config") configuration = message.result;
		return;
	}
	switch (method) {
		case "initialize":
			return send({
				id,
				result: {
					capabilities: {
						textDocumentSync: { openClose: true, change: 2, save: { includeText: false } },
						hoverProvider: true,
						definitionProvider: true,
						referencesProvider: true,
						documentFormattingProvider: true,
						documentSymbolProvider: true,
					},
					serverInfo: { name: "fake-lsp", version: "1.0.0" },
				},
			});
		case "initialized":
			return send({
				id: "config",
				method: "workspace/configuration",
				params: { items: [{ section: "fake" }] },
			});
		case "textDocument/didOpen":
			documents.set(params.textDocument.uri, params.textDocument.text);
			return publish(params.textDocument.uri);
		case "textDocument/didChange": {
			let text = documents.get(params.textDocument.uri) ?? "";
			for (const change of params.contentChanges) {
				if (!change.range) {
					text = change.text;
					continue;
				}
				const lines = text.split("\n");
				const offset = (p) =>
					lines.slice(0, p.line).reduce((sum, l) => sum + l.length + 1, 0) + p.character;
				text =
					text.slice(0, offset(change.range.start)) +
					change.text +
					text.slice(offset(change.range.end));
			}
			documents.set(params.textDocument.uri, text);
			return publish(params.textDocument.uri);
		}
		case "textDocument/didClose":
			documents.delete(params.textDocument.uri);
			return;
		case "textDocument/hover": {
			const word = wordAt(params.textDocument.uri, params.position);
			const greeting = configuration?.[0]?.greeting ?? "none";
			return send({
				id,
				result: { contents: { kind: "markdown", value: `**${word}** (${greeting})` } },
			});
		}
		case "textDocument/definition": {
			const word = wordAt(params.textDocument.uri, params.position);
			if (word === "outside" && process.env.FAKE_OUTSIDE) {
				return send({
					id,
					result: [
						{
							uri: `file://${process.env.FAKE_OUTSIDE}`,
							range: { start: { line: 0, character: 0 }, end: { line: 0, character: 3 } },
						},
					],
				});
			}
			return send({
				id,
				result: [
					{
						uri: params.textDocument.uri,
						range: { start: { line: 0, character: 3 }, end: { line: 0, character: 7 } },
					},
				],
			});
		}
		case "textDocument/references":
			return send({
				id,
				result: [...documents.keys()].map((uri) => ({
					uri,
					range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
				})),
			});
		case "textDocument/formatting": {
			const text = documents.get(params.textDocument.uri) ?? "";
			const lines = text.split("\n");
			const edits = [];
			lines.forEach((line, index) => {
				const trimmed = line.replace(/\s+$/, "");
				if (trimmed !== line) {
					edits.push({
						range: {
							start: { line: index, character: trimmed.length },
							end: { line: index, character: line.length },
						},
						newText: "",
					});
				}
			});
			return send({ id, result: edits });
		}
		case "textDocument/documentSymbol":
			return send({ id, result: [] });
		case "shutdown":
			return send({ id, result: null });
		case "exit":
			process.exit(0);
			return;
		default:
			if (id !== undefined) send({ id, error: { code: -32601, message: `no ${method}` } });
	}
}

process.stdin.on("data", (chunk) => {
	buffer = Buffer.concat([buffer, chunk]);
	for (;;) {
		const end = buffer.indexOf("\r\n\r\n");
		if (end === -1) return;
		const header = buffer.subarray(0, end).toString("utf8");
		const length = Number(/Content-Length: (\d+)/i.exec(header)?.[1] ?? 0);
		if (buffer.length < end + 4 + length) return;
		const body = buffer.subarray(end + 4, end + 4 + length).toString("utf8");
		buffer = buffer.subarray(end + 4 + length);
		handle(JSON.parse(body));
	}
});
process.stdin.on("end", () => process.exit(0));
