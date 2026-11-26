import { useBackground } from "@/modules/notifications/phone/hooks";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { Switch } from "@/shared/ui/switch";

/**
 * Settings → Notifications → Outside Ahab, in the Android app: the
 * permission, staying connected, and the battery hint.
 */
export function PhoneRows({ muted }: { muted: boolean }) {
	const { t } = useTranslation("notifications");
	const background = useBackground(true);
	const status = background.status;
	const row = `flex items-center gap-4 px-3 py-3.5 ${muted ? "opacity-60" : ""}`;
	return (
		<>
			<li className={row}>
				<div className="min-w-0 flex-1">
					<p className="text-heading text-sm font-semibold">{t("phone.title")}</p>
					<p className="text-muted-foreground text-sm text-pretty">
						{status?.permission === "granted"
							? status.running
								? t("phone.connected")
								: t("phone.allowed")
							: status?.permission === "denied"
								? t("phone.blocked")
								: t("phone.notAllowed")}
					</p>
					{background.error ? (
						<p role="alert" className="text-destructive text-sm">
							{background.error}
						</p>
					) : null}
				</div>
				{status && status.permission !== "granted" ? (
					<Button
						size="sm"
						variant="outline"
						onClick={() =>
							void background.ask({
								action: status.permission === "denied" ? "battery" : "allow",
							})
						}
					>
						{status.permission === "denied" ? t("phone.openSettings") : t("phone.allow")}
					</Button>
				) : null}
			</li>
			<li className={row}>
				<div className="min-w-0 flex-1">
					<p className="text-heading text-sm font-semibold">{t("phone.stayConnected.title")}</p>
					<p className="text-muted-foreground text-sm text-pretty">
						{t("phone.stayConnected.description")}
					</p>
				</div>
				<Switch
					label={t("phone.stayConnected.title")}
					checked={status?.stayConnected ?? false}
					disabled={!status}
					onCheckedChange={(stay) => void background.ask({ action: "stay", stay })}
				/>
			</li>
			<li className={row}>
				<div className="min-w-0 flex-1">
					<p className="text-heading text-sm font-semibold">{t("phone.late.title")}</p>
					<p className="text-muted-foreground text-sm text-pretty">{t("phone.late.description")}</p>
				</div>
				<Button
					size="sm"
					variant="ghost"
					onClick={() => void background.ask({ action: "battery" })}
				>
					{t("phone.late.action")}
				</Button>
			</li>
		</>
	);
}
