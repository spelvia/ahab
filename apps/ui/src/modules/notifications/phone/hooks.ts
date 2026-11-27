import { useCallback, useEffect, useState } from "react";

import { app, type BackgroundAsk, type BackgroundStatus } from "@/shared/desktop/bridge";

/** Whether this is the Android app, which keeps a background connection. */
export function usePhoneApp(): boolean {
	const [phone, setPhone] = useState(false);
	useEffect(() => {
		void app.info().then((info) => setPhone(info?.platform === "android"));
	}, []);
	return phone;
}

/** The phone's background connection: its state, and changing it. */
export function useBackground(phone: boolean) {
	const [status, setStatus] = useState<BackgroundStatus | undefined>();
	const [error, setError] = useState<string | undefined>();
	const ask = useCallback(async (asked: BackgroundAsk) => {
		try {
			const answer = await app.background(asked);
			if (answer) setStatus(answer);
			setError(undefined);
		} catch (failure) {
			setError(failure instanceof Error ? failure.message : String(failure));
		}
	}, []);
	useEffect(() => {
		if (!phone) return;
		let current = true;
		app
			.background({ action: "status" })
			.then((answer) => {
				if (current && answer) setStatus(answer);
			})
			.catch((failure: unknown) => {
				if (current) setError(failure instanceof Error ? failure.message : String(failure));
			});
		return () => {
			current = false;
		};
	}, [phone]);
	return { status, error, ask };
}

/** Tells the phone how many agents work, so it connects while some do. */
export function tellWorking(working: number) {
	void app.background({ action: "working", working }).catch(() => undefined);
}
