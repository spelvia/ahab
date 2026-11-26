import { cn } from "cn";
import { Check, ChevronDown } from "lucide-react";
import { createContext, useContext } from "react";

import { useTranslation } from "@/shared/i18n";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

export interface PickerOption {
	value: string;
	label: string;
	/** A line under the label, such as what a model is good at. */
	description?: string;
	/** Shown dimmed after the label, such as "default". */
	note?: string;
	/** Shown before the label, here and on the button when chosen. */
	icon?: React.ReactNode;
}

/**
 * Where pickers sit: in a toolbar (compact), or listed in a sheet on a
 * phone, one full-width row each, named, with its choice at the end.
 */
export const PickerPlacement = createContext<"toolbar" | "sheet">("toolbar");

/**
 * A compact choice in a toolbar: the chosen option as a small button that
 * opens the list (agent, model, reasoning effort).
 */
export function Picker({
	label,
	value,
	options,
	onChange,
	disabled,
	icon,
	className,
}: {
	/** What is being chosen, for assistive technology: "Model". */
	label: string;
	value: string | undefined;
	options: PickerOption[];
	onChange: (value: string) => void;
	disabled?: boolean;
	icon?: React.ReactNode;
	className?: string;
}) {
	const { t } = useTranslation();
	const placement = useContext(PickerPlacement);
	const chosen = options.find((option) => option.value === value);
	if (placement === "sheet")
		return (
			<DropdownMenu>
				<DropdownMenuTrigger
					disabled={disabled || options.length === 0}
					aria-label={t("picker.chosen", { label, value: chosen?.label ?? t("picker.none") })}
					className={cn(
						"hover:bg-accent data-[state=open]:bg-accent focus-visible:ring-ring flex min-h-12 w-full items-center gap-3 rounded-lg px-3 text-left text-sm outline-none focus-visible:ring-2 disabled:opacity-50 [&_svg]:size-4 [&_svg]:shrink-0",
						className,
					)}
				>
					<span className="text-muted-foreground">{chosen?.icon ?? icon}</span>
					<span className="text-foreground shrink-0">{label}</span>
					<span className="text-muted-foreground min-w-0 flex-1 truncate text-right">
						{chosen?.label ?? t("picker.none")}
					</span>
					<ChevronDown className="text-muted-foreground" />
				</DropdownMenuTrigger>
				<PickerOptions
					options={options}
					value={value}
					onChange={onChange}
					className="w-[var(--radix-dropdown-menu-trigger-width)]"
				/>
			</DropdownMenu>
		);
	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				disabled={disabled || options.length === 0}
				aria-label={t("picker.chosen", { label, value: chosen?.label ?? t("picker.none") })}
				className={cn(
					"text-muted-foreground hover:text-foreground hover:bg-accent data-[state=open]:bg-accent data-[state=open]:text-foreground focus-visible:ring-ring flex h-7 max-w-56 items-center gap-1.5 rounded-md px-2 text-xs outline-none focus-visible:ring-2 disabled:opacity-50 [&_svg]:size-3.5 [&_svg]:shrink-0",
					className,
				)}
			>
				{chosen?.icon ?? icon}
				<span className="truncate">{chosen?.label ?? label}</span>
				<ChevronDown className="opacity-60" />
			</DropdownMenuTrigger>
			<PickerOptions options={options} value={value} onChange={onChange} />
		</DropdownMenu>
	);
}

function PickerOptions({
	options,
	value,
	onChange,
	className,
}: {
	options: PickerOption[];
	value: string | undefined;
	onChange: (value: string) => void;
	className?: string;
}) {
	return (
		<DropdownMenuContent align="start" className={cn("max-h-80 w-72 overflow-auto", className)}>
			{options.map((option) => (
				<DropdownMenuItem
					key={option.value}
					onSelect={() => onChange(option.value)}
					className="items-start gap-2 py-1.5 max-sm:min-h-11 max-sm:py-2.5"
				>
					{option.icon ? <span className="mt-0.5 shrink-0">{option.icon}</span> : null}
					<span className="min-w-0 flex-1">
						<span className="flex items-baseline gap-1.5">
							<span className="text-foreground truncate">{option.label}</span>
							{option.note ? (
								<span className="text-muted-foreground text-[0.6875rem]">{option.note}</span>
							) : null}
						</span>
						{option.description ? (
							<span className="text-muted-foreground line-clamp-2 block text-xs">
								{option.description}
							</span>
						) : null}
					</span>
					{option.value === value ? <Check className="mt-0.5" /> : null}
				</DropdownMenuItem>
			))}
		</DropdownMenuContent>
	);
}
