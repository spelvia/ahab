import { QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";

import { queryClient } from "@/app/query-client";
import { router } from "@/app/router";
import { isBuiltinScreen, NativeApp } from "@/modules/native";
import { loadExternal, toolsLanguageService, TurnProblems } from "@/modules/tools";
import { registerAfterTurn } from "@/shared/chat-context/after-turn";
import {
	registerExternalFiles,
	registerHighlighter,
	registerLanguageService,
} from "@/shared/editor/registry";
import { syntaxHighlighter } from "@/shared/syntax/load";

// Highlighting in every editor: Tree-sitter in a worker.
registerHighlighter(syntaxHighlighter);
// Language servers on each project's machine.
registerLanguageService(toolsLanguageService);
registerExternalFiles(loadExternal);
// After an agent's turn, the problems it left.
registerAfterTurn(TurnProblems);

export function Providers() {
	// The desktop app's own copy of the web app, before a server is
	// connected: its first launch, servers and pairing.
	if (isBuiltinScreen()) return <NativeApp />;
	return (
		<QueryClientProvider client={queryClient}>
			<RouterProvider router={router} />
		</QueryClientProvider>
	);
}
