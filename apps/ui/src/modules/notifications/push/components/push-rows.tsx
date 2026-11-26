import { useRemoveSubscription, useTestPush } from "@/modules/notifications/push/api";
import { usePushHere } from "@/modules/notifications/push/hooks";
import type { PushSubscription as Subscribed } from "@/shared/api/generated/ahab.schemas";
import { formatDate, t as translate, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

function lastUsed(subscription: Subscribed) {
	if (!subscription.lastUsedAt) return translate("push.neverUsed", { ns: "notifications" });
	return translate("push.lastUsed", {
		ns: "notifications",
		when: formatDate(subscription.lastUsedAt, {
			day: "numeric",
			month: "short",
			hour: "2-digit",
			minute: "2-digit",
		}),
	});
}

/**
 * Settings → Notifications → Outside Ahab, in a browser: this browser's Web
 * Push (on, off, a test), then the other browsers that get pushes.
 */
export function PushRows({
	muted,
	permission,
}: {
	muted: boolean;
	/** The in-tab fallback: this browser's notifications while a tab is open. */
	permission: { supported: boolean; state: NotificationPermission; ask: () => Promise<void> };
}) {
	const { t } = useTranslation(["notifications", "common"]);
	const push = usePushHere();
	const test = useTestPush();
	const remove = useRemoveSubscription();
	const testing = test.isPending;
	const outcome = test.data?.outcome;

	const description =
		push.supported === "insecure"
			? permission.state === "granted"
				? t("push.insecureAllowed")
				: t("push.insecure")
			: push.supported === "unsupported"
				? permission.supported
					? t("push.tabOnly")
					: t("push.unsupported")
				: push.state === "blocked"
					? t("push.blocked")
					: push.state === "on"
						? outcome === "sent"
							? t("push.testSent")
							: outcome === "failed"
								? t("push.testFailed")
								: t("push.on")
						: t("push.off");

	return (
		<>
			<li className={`flex items-center gap-4 px-3 py-3.5 ${muted ? "opacity-60" : ""}`}>
				<div className="min-w-0 flex-1">
					<p className="text-heading text-sm font-semibold">{t("push.title")}</p>
					<p className="text-muted-foreground text-sm text-pretty">{description}</p>
					{push.error ? (
						<p role="alert" className="text-destructive text-sm">
							{push.error}
						</p>
					) : null}
				</div>
				{push.supported === "supported" ? (
					push.state === "on" && push.mine ? (
						<div className="flex gap-2">
							<Button
								size="sm"
								variant="outline"
								disabled={testing}
								onClick={() => {
									if (push.mine) test.mutate({ id: push.mine.id });
								}}
							>
								{t("push.sendTest")}
							</Button>
							<Button size="sm" variant="ghost" onClick={() => void push.turnOff()}>
								{t("push.turnOff")}
							</Button>
						</div>
					) : push.state === "off" || push.state === "working" ? (
						<Button
							size="sm"
							variant="outline"
							disabled={push.state === "working"}
							onClick={() => void push.turnOn()}
						>
							{t("push.turnOn")}
						</Button>
					) : null
				) : permission.supported && permission.state === "default" ? (
					<Button size="sm" variant="outline" onClick={() => void permission.ask()}>
						{t("push.allow")}
					</Button>
				) : null}
			</li>
			{push.others.map((subscription) => (
				<li
					key={subscription.id}
					className={`flex items-center gap-4 px-3 py-3.5 ${muted ? "opacity-60" : ""}`}
				>
					<div className="min-w-0 flex-1">
						<p className="text-heading text-sm font-semibold">{subscription.label}</p>
						<p className="text-muted-foreground text-sm">
							{t("push.other", {
								where: subscription.thisClient ? t("push.thisDevice") : t("push.anotherDevice"),
								lastUsed: lastUsed(subscription),
							})}
						</p>
					</div>
					<Button
						size="sm"
						variant="ghost"
						disabled={remove.isPending}
						onClick={() => remove.mutate({ id: subscription.id })}
					>
						{t("common:actions.remove")}
					</Button>
				</li>
			))}
		</>
	);
}
