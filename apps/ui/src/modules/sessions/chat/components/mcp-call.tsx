import { Plug } from "lucide-react";
import type { ReactNode } from "react";

import { Fields } from "@/modules/sessions/chat/components/permission-request";
import { inputFields, type McpResult } from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";
import { HighlightedCode } from "@/shared/syntax/code";

/**
 * An MCP tool call, opened: its arguments as names and values, then what
 * the server returned (its text, structured content and other blocks) or
 * its error.
 */
export function McpCallBody({
	arguments: args,
	result,
	error,
	running,
}: {
	arguments: unknown;
	result?: McpResult | null;
	error?: string | null;
	running: boolean;
}) {
	const { t } = useTranslation("sessions");
	const fields = inputFields(args);
	const blocks = result?.content ?? [];
	const structured = result?.structuredContent;
	return (
		<div className="flex flex-col gap-3 text-xs">
			<Section title={t("mcp.arguments")}>
				{fields.length ? (
					<Fields fields={fields} label={t("mcp.arguments")} />
				) : (
					<p className="text-muted-foreground">{t("mcp.noArguments")}</p>
				)}
			</Section>
			{error ? (
				<Section title={t("mcp.error")}>
					<pre className="bg-destructive/8 text-destructive overflow-auto rounded-md p-2.5 font-mono leading-5 whitespace-pre-wrap">
						{error}
					</pre>
				</Section>
			) : null}
			{blocks.length ? (
				<Section title={t("mcp.result")}>
					<div className="flex flex-col gap-1.5">
						{blocks.map((block, index) => (
							// Blocks have no identity of their own; their order is theirs.
							// oxlint-disable-next-line react/no-array-index-key
							<Block key={index} block={block} />
						))}
					</div>
				</Section>
			) : null}
			{structured !== undefined && structured !== null ? (
				<Section title={t("mcp.structured")}>
					<pre className="bg-muted/60 max-h-80 overflow-auto rounded-md p-2.5 font-mono leading-5">
						<HighlightedCode text={JSON.stringify(structured, null, 2)} as={{ language: "json" }} />
					</pre>
				</Section>
			) : null}
			{!error && !blocks.length && (structured === undefined || structured === null) && !running ? (
				<p className="text-muted-foreground">{t("mcp.noResult")}</p>
			) : null}
		</div>
	);
}

function Section({ title, children }: { title: string; children: ReactNode }) {
	return (
		<section className="flex flex-col gap-1">
			<h4 className="text-muted-foreground flex items-center gap-1.5 text-xs font-medium">
				{title}
			</h4>
			{children}
		</section>
	);
}

/** One content block: text as text, the others named. */
function Block({ block }: { block: NonNullable<McpResult["content"]>[number] }) {
	const { t } = useTranslation("sessions");
	if (block.type === "text" || typeof block.text === "string")
		return (
			<pre className="bg-muted/60 max-h-80 overflow-auto rounded-md p-2.5 font-mono leading-5 whitespace-pre-wrap">
				{block.text}
			</pre>
		);
	const type = typeof block.type === "string" ? block.type : "?";
	const mime = typeof block.mimeType === "string" ? block.mimeType : type;
	const resource = block.resource as { uri?: string } | undefined;
	const uri = typeof block.uri === "string" ? block.uri : resource?.uri;
	return (
		<p className="text-muted-foreground flex items-center gap-1.5">
			<Plug className="size-3.5" aria-hidden />
			{type === "image"
				? t("mcp.image", { type: mime })
				: uri
					? t("mcp.resource", { uri })
					: t("mcp.block", { type })}
		</p>
	);
}
