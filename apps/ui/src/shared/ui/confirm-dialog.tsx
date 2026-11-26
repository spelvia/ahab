import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";

/** Asks before something that stops work or cannot be undone. */
export function ConfirmDialog({
	open,
	onOpenChange,
	title,
	description,
	action,
	destructive,
	pending,
	error,
	onConfirm,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	title: string;
	description: string;
	action: string;
	destructive?: boolean;
	pending?: boolean;
	error?: string;
	onConfirm: () => void;
}) {
	const { t } = useTranslation();
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent size="sm" role="alertdialog">
				<DialogHeader className={error ? undefined : "pb-6"}>
					<DialogTitle>{title}</DialogTitle>
					<DialogDescription>{description}</DialogDescription>
				</DialogHeader>
				{error ? (
					<DialogBody>
						<p role="alert" className="text-destructive text-sm">
							{error}
						</p>
					</DialogBody>
				) : null}
				<DialogFooter>
					<Button variant="ghost" onClick={() => onOpenChange(false)}>
						{t("actions.cancel")}
					</Button>
					<Button
						variant={destructive ? "destructive" : "default"}
						disabled={pending}
						onClick={onConfirm}
					>
						{action}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
