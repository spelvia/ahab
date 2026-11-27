import { Check, Copy, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";

import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { copyText } from "@/shared/ui/clipboard";

/** Copies `text`, and says for two seconds whether it worked. */
export function CopyButton({ text, label }: { text: string; label: string }) {
	const { t } = useTranslation();
	const [result, setResult] = useState<"copied" | "failed">();
	useEffect(() => {
		if (!result) return;
		const timer = setTimeout(() => setResult(undefined), 2_500);
		return () => clearTimeout(timer);
	}, [result]);

	const description =
		result === "copied"
			? t("copyButton.copied", { label })
			: result === "failed"
				? t("copyButton.failed", { label })
				: t("copyButton.copy", { label });

	return (
		<Button
			type="button"
			variant="ghost"
			size="icon-xs"
			aria-label={description}
			title={description}
			onClick={() => void copyText(text).then((ok) => setResult(ok ? "copied" : "failed"))}
		>
			{result === "copied" ? (
				<Check className="text-foreground" />
			) : result === "failed" ? (
				<TriangleAlert className="text-destructive" />
			) : (
				<Copy />
			)}
		</Button>
	);
}
