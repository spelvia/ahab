import { useVimStatus, vimModeLabel } from "@/shared/editor/vim";
import { useTranslation } from "@/shared/i18n";
import { StatusText } from "@/shared/ui/status-bar";

/**
 * The status bar's Vim item, as Vim's status line shows it: `-- INSERT --`,
 * and the keys of a command being typed (`d2`) beside it. Nothing while the
 * focused editor has no Vim.
 */
export function VimStatusItem() {
	const { t } = useTranslation();
	const status = useVimStatus();
	if (!status) return null;
	const mode = vimModeLabel(status.mode);
	return (
		<StatusText
			title={
				status.pending
					? t("editor.vim.modeWithKeys", { mode, keys: status.pending })
					: t("editor.vim.mode", { mode })
			}
			className="gap-2 font-mono"
		>
			<span
				role="status"
				aria-live="polite"
				className={status.mode === "normal" ? undefined : "text-foreground"}
			>
				{t("editor.vim.modeShown", { mode })}
			</span>
			{status.pending ? <span className="text-foreground">{status.pending}</span> : null}
		</StatusText>
	);
}
