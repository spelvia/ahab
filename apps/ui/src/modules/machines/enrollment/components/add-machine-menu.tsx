import {
	ChevronDown,
	ExternalLink,
	KeyRound,
	Plus,
	ShieldCheck,
	SquareTerminal,
} from "lucide-react";

import { docs } from "@/shared/docs";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

/**
 * "Add machine": the ways another computer can join this server, over SSH
 * (recommended) first.
 */
export function AddMachineMenu({ onSsh, onCode }: { onSsh: () => void; onCode: () => void }) {
	const { t } = useTranslation("machines");
	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button>
					<Plus />
					{t("addMachineMenu.button")}
					<ChevronDown className="opacity-70" />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent className="w-80">
				<DropdownMenuItem onSelect={onSsh} className="items-start py-2">
					<SquareTerminal className="mt-0.5" />
					<Option title={t("addMachineMenu.ssh")} text={t("addMachineMenu.sshText")} />
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={onCode} className="items-start py-2">
					<KeyRound className="mt-0.5" />
					<Option title={t("addMachineMenu.code")} text={t("addMachineMenu.codeText")} />
				</DropdownMenuItem>
				<DropdownMenuItem asChild className="items-start py-2">
					<a
						href={docs("/installation/cli/#connect-another-machine")}
						target="_blank"
						rel="noreferrer"
					>
						<ShieldCheck className="mt-0.5" />
						<Option
							title={t("addMachineMenu.approval")}
							text={t("addMachineMenu.approvalText")}
							external
						/>
					</a>
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

function Option({ title, text, external }: { title: string; text: string; external?: boolean }) {
	const { t } = useTranslation("machines");
	return (
		<span className="flex min-w-0 flex-col gap-0.5">
			<span className="text-heading flex items-center gap-1.5 font-medium">
				{title}
				{external && (
					<ExternalLink className="size-3!" aria-label={t("addMachineMenu.opensDocs")} />
				)}
			</span>
			<span className="text-muted-foreground text-xs leading-snug">{text}</span>
		</span>
	);
}
