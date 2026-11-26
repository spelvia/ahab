import { cn } from "cn";
import { useState } from "react";

import { useUpdateSession } from "@/modules/sessions/session/api";
import type { Session } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";

/** Renames a session in place: Enter or leaving saves, Escape cancels. */
export function TitleField({
	session,
	onDone,
	className,
}: {
	session: Session;
	onDone: () => void;
	className?: string;
}) {
	const { t } = useTranslation("sessions");
	const update = useUpdateSession();
	const [title, setTitle] = useState(session.title);
	const save = () => {
		const next = title.trim();
		if (next && next !== session.title) update.mutate({ id: session.id, data: { title: next } });
		onDone();
	};
	return (
		<input
			aria-label={t("titleField.label")}
			autoFocus
			value={title}
			maxLength={120}
			onChange={(event) => setTitle(event.target.value)}
			onFocus={(event) => event.target.select()}
			onBlur={save}
			onClick={(event) => event.stopPropagation()}
			onKeyDown={(event) => {
				event.stopPropagation();
				if (event.key === "Enter") save();
				if (event.key === "Escape") onDone();
			}}
			className={cn(
				"bg-background border-ring text-heading min-w-0 flex-1 rounded border px-1.5 py-0.5 text-sm outline-none",
				className,
			)}
		/>
	);
}
