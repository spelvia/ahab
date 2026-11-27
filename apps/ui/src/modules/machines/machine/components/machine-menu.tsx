import { Copy, Ellipsis, Pencil, Trash2 } from "lucide-react";
import { useRef } from "react";

import type { Machine } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { copyText } from "@/shared/ui/clipboard";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { toast } from "@/shared/ui/toast";

/** A machine's actions behind ⋯: rename, copy its fingerprint, remove. */
export function MachineMenu({
	machine,
	onRename,
	onRemove,
}: {
	machine: Machine;
	onRename: () => void;
	onRemove: () => void;
}) {
	// Rename and remove move focus into the row; the menu must not take it
	// back to its trigger when it closes.
	const keepFocus = useRef(false);
	const { t } = useTranslation("machines");
	const { t: common } = useTranslation();
	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="ghost" size="icon-sm" aria-label={t("menu.label", { name: machine.name })}>
					<Ellipsis />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent
				onCloseAutoFocus={(event) => {
					if (keepFocus.current) event.preventDefault();
					keepFocus.current = false;
				}}
			>
				<DropdownMenuItem
					onSelect={() => {
						keepFocus.current = true;
						onRename();
					}}
				>
					<Pencil />
					{t("menu.rename")}
				</DropdownMenuItem>
				<DropdownMenuItem
					onSelect={() => {
						void copyText(machine.fingerprint).then((ok) =>
							toast(
								ok
									? t("menu.fingerprintCopied", { name: machine.name })
									: t("menu.fingerprintNotCopied", { name: machine.name }),
							),
						);
					}}
				>
					<Copy />
					{t("menu.copyFingerprint")}
				</DropdownMenuItem>
				{!machine.isLocal && (
					<>
						<DropdownMenuSeparator />
						<DropdownMenuItem
							variant="destructive"
							onSelect={() => {
								keepFocus.current = true;
								onRemove();
							}}
						>
							<Trash2 />
							{machine.state === "pending" ? t("menu.reject") : common("actions.remove")}
						</DropdownMenuItem>
					</>
				)}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
