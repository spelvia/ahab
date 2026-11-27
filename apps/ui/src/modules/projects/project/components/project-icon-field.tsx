import { ImageUp, Smile, X } from "lucide-react";
import { useRef, useState } from "react";

import { useRemoveProjectIcon, useSetProjectIcon } from "@/modules/projects/project/api";
import { ProjectMark } from "@/modules/projects/project/components/project-switcher";
import type { Project } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { toastError } from "@/shared/ui/toast";

/** The most the server keeps of an image (256 KiB). */
const MOST_BYTES = 256 * 1024;
/** Images are made this many pixels square before they are sent. */
const SIDE = 128;

/** Reads a file as a `data:` URL. */
function dataUrl(file: Blob): Promise<string> {
	return new Promise((resolve, reject) => {
		const reader = new FileReader();
		reader.addEventListener("load", () => resolve(String(reader.result)));
		reader.addEventListener("error", () => reject(reader.error ?? new Error("unreadable")));
		reader.readAsDataURL(file);
	});
}

/**
 * An image as a project's mark: a small GIF as it is (it may move), anything
 * else cropped to its middle square and made 128 pixels, as PNG.
 */
async function markImage(file: File): Promise<string> {
	if (file.type === "image/gif" && file.size <= MOST_BYTES) return dataUrl(file);
	const bitmap = await createImageBitmap(file);
	const side = Math.min(bitmap.width, bitmap.height);
	const canvas = document.createElement("canvas");
	canvas.width = SIDE;
	canvas.height = SIDE;
	const context = canvas.getContext("2d");
	if (!context) throw new Error("This browser cannot resize images.");
	context.drawImage(
		bitmap,
		(bitmap.width - side) / 2,
		(bitmap.height - side) / 2,
		side,
		side,
		0,
		0,
		SIDE,
		SIDE,
	);
	bitmap.close();
	return canvas.toDataURL("image/png");
}

/** A project's mark: an emoji, an image, or its initial. */
export function ProjectIconField({ project }: { project: Project }) {
	const { t } = useTranslation("projects");
	const set = useSetProjectIcon();
	const remove = useRemoveProjectIcon();
	const [emoji, setEmoji] = useState(project.icon?.emoji ?? "");
	const picker = useRef<HTMLInputElement>(null);
	const pending = set.isPending || remove.isPending;
	const error = set.error?.message ?? remove.error?.message;
	return (
		<div className="space-y-1.5">
			<span className="text-muted-foreground text-xs font-medium">
				{t("projectPage.icon.label")}
			</span>
			<div className="flex flex-wrap items-center gap-2">
				<ProjectMark
					name={project.name}
					icon={project.icon}
					className="size-9 rounded-lg text-base"
				/>
				<form
					className="flex items-center gap-1.5"
					onSubmit={(event) => {
						event.preventDefault();
						if (!emoji.trim()) return;
						set.mutate({ project: project.id, data: { emoji: emoji.trim() } });
					}}
				>
					<input
						value={emoji}
						maxLength={16}
						onChange={(event) => setEmoji(event.target.value)}
						aria-label={t("projectPage.icon.emoji")}
						placeholder="🐋"
						className="border-input bg-background focus-visible:ring-ring h-9 w-20 rounded-md border px-2.5 text-center text-sm outline-none focus-visible:ring-2"
					/>
					<Button
						type="submit"
						variant="secondary"
						disabled={pending || !emoji.trim() || emoji.trim() === project.icon?.emoji}
					>
						<Smile className="size-4" />
						{t("projectPage.icon.useEmoji")}
					</Button>
				</form>
				<Button
					type="button"
					variant="secondary"
					disabled={pending}
					onClick={() => picker.current?.click()}
				>
					<ImageUp className="size-4" />
					{t("projectPage.icon.upload")}
				</Button>
				<input
					ref={picker}
					type="file"
					accept="image/png,image/jpeg,image/webp,image/gif"
					hidden
					onChange={(event) => {
						const file = event.target.files?.[0];
						event.target.value = "";
						if (!file) return;
						void markImage(file).then(
							(image) => set.mutate({ project: project.id, data: { image } }),
							(failed: unknown) =>
								toastError(failed instanceof Error ? failed : new Error(String(failed))),
						);
					}}
				/>
				{project.icon ? (
					<Button
						type="button"
						variant="ghost"
						disabled={pending}
						onClick={() => {
							setEmoji("");
							remove.mutate({ project: project.id });
						}}
					>
						<X className="size-4" />
						{t("projectPage.icon.remove")}
					</Button>
				) : null}
			</div>
			<p className="text-muted-foreground text-xs">{t("projectPage.icon.hint")}</p>
			{error ? (
				<p role="alert" className="text-destructive text-sm">
					{error}
				</p>
			) : null}
		</div>
	);
}
