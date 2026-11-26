import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { native } from "@/modules/native/shell/api";
import { Connect } from "@/modules/native/shell/pages/connect";
import { LinkScreen } from "@/modules/native/shell/pages/link";
import { Running } from "@/modules/native/shell/pages/running";
import { SettingsScreen } from "@/modules/native/shell/pages/settings";
import { Start } from "@/modules/native/shell/pages/start";
import { Welcome } from "@/modules/native/shell/pages/welcome";
import { boot, computer, mockNative } from "@/testing/native";

vi.mock("@/modules/native/shell/api", async (original) => {
	const actual = await original<typeof import("@/modules/native/shell/api")>();
	return { ...actual, native: { ...actual.native } };
});

const calls = native as unknown as Record<string, ReturnType<typeof vi.fn>>;

beforeEach(() => {
	mockNative(native);
	window.location.hash = "";
});

describe("the start screen", () => {
	it("welcomes on the first launch", async () => {
		calls.boot!.mockResolvedValue(boot());
		render(<Start />);
		await waitFor(() => expect(window.location.hash).toBe("#/welcome"));
	});

	it("uses the Ahab already running here, without asking", async () => {
		calls.boot!.mockResolvedValue(boot({ computer: computer({ role: "server", running: true }) }));
		render(<Start />);
		await waitFor(() => expect(window.location.hash).toBe("#/running?action=existing"));
	});

	it("welcomes when ahab is installed here but not running", async () => {
		calls.boot!.mockResolvedValue(boot({ computer: computer({ role: "server", running: false }) }));
		render(<Start />);
		await waitFor(() => expect(window.location.hash).toBe("#/welcome"));
	});

	it("pairs with the server this computer is a machine of", async () => {
		calls.boot!.mockResolvedValue(
			boot({ computer: computer({ role: "machine", joined: "https://atlas.lan" }) }),
		);
		render(<Start />);
		await waitFor(() =>
			expect(window.location.hash).toBe("#/connect?address=https%3A%2F%2Fatlas.lan"),
		);
	});

	it("opens the last server", async () => {
		const atlas = { id: "s2", name: "atlas", url: "https://atlas.lan", local: false };
		calls.boot!.mockResolvedValue(
			boot({
				servers: {
					servers: [{ id: "s1", name: "x", url: "https://x", local: false }, atlas],
					last: "s2",
					store: "keychain",
				},
			}),
		);
		render(<Start />);
		await waitFor(() => expect(window.location.hash).toBe("#/opening?server=s2"));
	});
});

describe("the first launch", () => {
	it("has one way forward, and a quiet way to a server elsewhere", async () => {
		calls.thisComputer!.mockResolvedValue(computer());
		render(<Welcome />);
		expect(
			screen.getByRole("heading", { name: "Your agents, on your machines." }),
		).toBeInTheDocument();
		const start = await screen.findByRole("button", { name: "Start Ahab on this computer" });
		await waitFor(() => expect(start).toBeEnabled());
		await userEvent.click(screen.getByRole("button", { name: /I already have an Ahab server/ }));
		expect(window.location.hash).toBe("#/connect");
		await userEvent.click(start);
		expect(window.location.hash).toBe("#/running?action=run");
		expect(screen.queryByRole("radio")).toBeNull();
	});

	it("on a phone, only connects: it never runs Ahab", async () => {
		const agent = vi
			.spyOn(window.navigator, "userAgent", "get")
			.mockReturnValue(
				"Mozilla/5.0 (Linux; Android 16; sdk_gphone64_x86_64) AppleWebKit/537.36 Version/4.0 Chrome/140.0 Mobile Safari/537.36",
			);
		const phone = vi
			.spyOn(await import("@/modules/native/shell/hooks"), "isPhoneApp")
			.mockReturnValue(true);
		calls.thisComputer!.mockResolvedValue(computer({ bundled: null, serviceManager: null }));
		render(<Welcome />);
		expect(screen.queryByRole("button", { name: "Start Ahab on this computer" })).toBeNull();
		expect(screen.queryByText(/no Ahab to run/)).toBeNull();
		await userEvent.click(screen.getByRole("button", { name: "Connect to your Ahab server" }));
		expect(window.location.hash).toBe("#/connect");
		agent.mockRestore();
		phone.mockRestore();
	});

	it("connects instead when Ahab cannot run here", async () => {
		calls.thisComputer!.mockResolvedValue(computer({ serviceManager: null }));
		render(<Welcome />);
		expect(await screen.findByText(/no service manager/)).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "Start Ahab on this computer" })).toBeNull();
		await userEvent.click(screen.getByRole("button", { name: "Connect to an Ahab server" }));
		expect(window.location.hash).toBe("#/connect");
	});
});

describe("starting Ahab here", () => {
	it("ticks off what really happens, shows the agents found, then opens Ahab", async () => {
		const here = { id: "s1", name: "thinkpad", url: "http://127.0.0.1:8180", local: true };
		calls.runHere!.mockResolvedValue(here);
		calls.agentsHere!.mockResolvedValue({
			machine: "thinkpad",
			online: true,
			agents: [
				{ agent: "claude-code", label: "Claude Code", state: "ready" },
				{ agent: "codex", label: "Codex", state: "notSignedIn" },
				{ agent: "pi", label: "Pi", state: "notInstalled" },
			],
		});
		render(<Running action="run" />);
		expect(await screen.findByText("Ahab is running on thinkpad")).toBeInTheDocument();
		expect(await screen.findByText("Claude Code")).toBeInTheDocument();
		expect(screen.getByText("sign in")).toBeInTheDocument();
		expect(screen.queryByText("Pi")).toBeNull();
		await waitFor(() => expect(calls.openServer).toHaveBeenCalledWith("s1"), { timeout: 5000 });
		expect(screen.getByRole("heading", { name: "Ahab is ready." })).toBeInTheDocument();
	});
});

describe("connecting to a server", () => {
	it("shows the app's code to approve, and pairs with a typed code", async () => {
		calls.reach!.mockResolvedValue({ url: "https://atlas.lan", version: "0.1.0", known: null });
		calls.ask!.mockResolvedValue({ id: "r1", code: "7F3K" });
		calls.wait!.mockImplementation(() => new Promise(() => {}));
		calls.pair!.mockResolvedValue({
			id: "s1",
			name: "atlas.lan",
			url: "https://atlas.lan",
			local: false,
		});
		render(<Connect address="" machine={false} />);
		await userEvent.type(screen.getByLabelText("Server address"), "atlas.lan");
		await userEvent.click(screen.getByRole("button", { name: "Continue" }));
		expect(await screen.findByLabelText("Pairing code")).toHaveTextContent("7F3K");
		await userEvent.type(screen.getByLabelText(/code Add client shows/), "K7QM-4RXT");
		await userEvent.click(screen.getByRole("button", { name: "Pair" }));
		await waitFor(() =>
			expect(calls.pair).toHaveBeenCalledWith("https://atlas.lan", undefined, "K7QM-4RXT"),
		);
		expect(calls.openServer).toHaveBeenCalledWith("s1");
	});

	it("says when the address is not a server", async () => {
		calls.reach!.mockRejectedValue({ code: "unreachable", message: "Nothing answers there." });
		render(<Connect address="" machine={false} />);
		await userEvent.type(screen.getByLabelText("Server address"), "nowhere");
		await userEvent.click(screen.getByRole("button", { name: "Continue" }));
		expect(await screen.findByRole("alert")).toHaveTextContent("Nothing answers there.");
	});
});

describe("ahab:// links", () => {
	it("pair only once the person confirms", async () => {
		calls.takeLink!.mockResolvedValue("ahab://pair?server=https://evil.example&secret=s");
		calls.parseLink!.mockResolvedValue({
			kind: "pair",
			server: "https://evil.example",
			secret: "s",
		});
		calls.boot!.mockResolvedValue(boot());
		calls.pair!.mockResolvedValue({
			id: "s9",
			name: "evil.example",
			url: "https://evil.example",
			local: false,
		});
		render(<LinkScreen />);
		expect(await screen.findByText("https://evil.example")).toBeInTheDocument();
		expect(calls.pair).not.toHaveBeenCalled();
		await userEvent.click(screen.getByRole("button", { name: "Pair" }));
		await waitFor(() =>
			expect(calls.pair).toHaveBeenCalledWith("https://evil.example", "s", undefined),
		);
	});

	it("join with the server's fingerprint shown, after confirming", async () => {
		calls.takeLink!.mockResolvedValue("ahab://join?…");
		calls.parseLink!.mockResolvedValue({
			kind: "join",
			server: "https://atlas.lan",
			code: "ABCD-EFGH",
			fingerprint: "SHA256:abc",
		});
		calls.boot!.mockResolvedValue(boot());
		render(<LinkScreen />);
		expect(await screen.findByText("SHA256:abc")).toBeInTheDocument();
		expect(calls.joinWithCode).not.toHaveBeenCalled();
		await userEvent.click(screen.getByRole("button", { name: "Add this computer" }));
		await waitFor(() =>
			expect(calls.joinWithCode).toHaveBeenCalledWith(
				"https://atlas.lan",
				"ABCD-EFGH",
				"SHA256:abc",
			),
		);
	});

	it("open a known server's page directly", async () => {
		calls.parseLink!.mockResolvedValue({ kind: "open", server: "https://atlas.lan", path: "/p/x" });
		calls.boot!.mockResolvedValue(
			boot({
				servers: {
					servers: [{ id: "s1", name: "atlas", url: "https://atlas.lan", local: false }],
					store: "keychain",
				},
			}),
		);
		render(<LinkScreen url="ahab://open?server=https://atlas.lan&path=/p/x" />);
		await waitFor(() => expect(calls.openServer).toHaveBeenCalledWith("s1", "/p/x"));
	});
});

describe("App settings → This computer", () => {
	it("asks before stopping, saying what stops", async () => {
		calls.boot!.mockResolvedValue(
			boot({ computer: computer({ role: "server", running: true, url: "http://127.0.0.1:8180" }) }),
		);
		calls.checkForUpdates!.mockResolvedValue({
			enabled: true,
			available: false,
			note: "Not published yet.",
		});
		calls.stopHere!.mockResolvedValue(computer({ role: "server" }));
		render(<SettingsScreen confirmStop={false} />);
		await userEvent.click(
			await screen.findByRole("button", { name: "Stop Ahab on this computer" }),
		);
		expect(calls.stopHere).not.toHaveBeenCalled();
		expect(screen.getByRole("alertdialog")).toHaveTextContent(
			/Agents and terminals on this computer stop/,
		);
		await userEvent.click(screen.getByRole("button", { name: "Stop" }));
		await waitFor(() => expect(calls.stopHere).toHaveBeenCalled());
	});

	it("leaves a service the system configuration manages alone", async () => {
		calls.boot!.mockResolvedValue(
			boot({ computer: computer({ role: "server", running: true, managed: true }) }),
		);
		render(<SettingsScreen confirmStop={false} />);
		expect(await screen.findByText(/start and stop it there/)).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: /Stop Ahab/ })).toBeNull();
	});

	it("shows the update note while releases have no home", async () => {
		calls.boot!.mockResolvedValue(boot());
		calls.checkForUpdates!.mockResolvedValue({
			enabled: true,
			available: false,
			note: "Not published yet.",
		});
		render(<SettingsScreen confirmStop={false} />);
		expect(await screen.findByText("Not published yet.")).toBeInTheDocument();
	});
});
