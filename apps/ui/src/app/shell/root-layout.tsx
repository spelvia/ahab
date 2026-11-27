import { Link, Outlet } from "@tanstack/react-router";

import { PairingGate, PairingPrompts } from "@/modules/clients";
import { NotificationAlerts } from "@/modules/notifications";
import { PluginLoader } from "@/modules/plugins";
import { useSessionUpdates } from "@/modules/sessions";
import { useTranslation } from "@/shared/i18n";
import { SplashDone } from "@/shared/splash/splash-done";
import { Button } from "@/shared/ui/button";
import { Toaster } from "@/shared/ui/toast";

/**
 * Every page. The workbench and Settings draw their own frames. A browser
 * that is not paired sees the pairing screen instead.
 */
export function RootLayout() {
	return (
		<>
			<PairingGate>
				<Outlet />
				<PluginLoader />
				<SessionUpdates />
			</PairingGate>
			<PairingPrompts />
			<NotificationAlerts />
			<Toaster />
		</>
	);
}

function SessionUpdates() {
	useSessionUpdates();
	return null;
}

/** An address no page has. */
export function NotFound() {
	const { t } = useTranslation("shell");
	return (
		<section className="mx-auto max-w-lg px-6 pt-24">
			<SplashDone />
			<h1 className="font-heading text-heading text-2xl font-semibold">{t("notFound.title")}</h1>
			<Button asChild variant="outline" className="mt-4">
				<Link to="/">{t("backToProjects")}</Link>
			</Button>
		</section>
	);
}
