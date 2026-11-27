import { useState } from "react";

import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import {
	Dialog,
	DialogBody,
	DialogClose,
	DialogContent,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";

/** Asks for a name, such as a new folder's. */
export function NameDialog({
	open,
	onOpenChange,
	title,
	label,
	initial = "",
	action,
	pending,
	error,
	onSubmit,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	title: string;
	label: string;
	initial?: string;
	action: string;
	pending?: boolean;
	error?: string;
	onSubmit: (name: string) => void;
}) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent size="sm">
				<DialogHeader>
					<DialogTitle>{title}</DialogTitle>
				</DialogHeader>
				{/* Remounted each time it opens, so it starts from `initial`. */}
				{open ? (
					<NameForm
						label={label}
						initial={initial}
						action={action}
						pending={pending}
						error={error}
						onSubmit={onSubmit}
					/>
				) : null}
			</DialogContent>
		</Dialog>
	);
}

function NameForm({
	label,
	initial,
	action,
	pending,
	error,
	onSubmit,
}: {
	label: string;
	initial: string;
	action: string;
	pending?: boolean;
	error?: string;
	onSubmit: (name: string) => void;
}) {
	const [name, setName] = useState(initial);
	const { t } = useTranslation();
	return (
		<form
			className="flex min-h-0 flex-col"
			onSubmit={(event) => {
				event.preventDefault();
				if (name.trim()) onSubmit(name.trim());
			}}
		>
			<DialogBody>
				<label className="text-muted-foreground text-xs font-medium">
					{label}
					<input
						autoFocus
						value={name}
						maxLength={60}
						onChange={(event) => setName(event.target.value)}
						className="bg-background border-input text-foreground focus-visible:ring-ring mt-1.5 block h-9 w-full rounded-md border px-2.5 text-sm outline-none focus-visible:ring-2"
					/>
				</label>
				{error ? (
					<p role="alert" className="text-destructive text-sm">
						{error}
					</p>
				) : null}
			</DialogBody>
			<DialogFooter>
				<DialogClose asChild>
					<Button type="button" variant="ghost">
						{t("actions.cancel")}
					</Button>
				</DialogClose>
				<Button type="submit" disabled={pending || !name.trim()}>
					{action}
				</Button>
			</DialogFooter>
		</form>
	);
}
