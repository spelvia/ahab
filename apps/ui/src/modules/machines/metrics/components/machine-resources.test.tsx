import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { MachineResources } from "@/modules/machines/metrics/components/machine-resources";
import type { MachineMetrics, ProcessMetrics } from "@/shared/api/generated/ahab.schemas";
import { getAgentCatalogMockHandler } from "@/shared/api/generated/machines-agents/machines-agents.msw";
import { getMachineMetricsMockHandler } from "@/shared/api/generated/machines-metrics/machines-metrics.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

const GB = 1024 ** 3;

function process(pid: number, name: string, cpu: number, ahab = false): ProcessMetrics {
	return {
		pid,
		name,
		cpuPercent: cpu,
		memoryBytes: 100 * 1024 ** 2,
		ahab,
		script: null,
		role: ahab ? "ahab" : null,
		parent: null,
		user: "alex",
		status: "Sleep",
		threads: 4,
		cpuTimeMs: 61_000,
		runSeconds: 3_600,
		virtualBytes: 1024 ** 3,
		readPerSecond: 0,
		writtenPerSecond: 2048,
		executable: `/usr/bin/${name}`,
		cwd: "/home/alex",
	};
}

const metrics: MachineMetrics = {
	sampledAt: new Date().toISOString(),
	hostName: "atlas",
	osVersion: "Linux (NixOS 26.11)",
	kernel: "6.18",
	uptimeSeconds: 3600,
	cpu: { usagePercent: 12, cores: [10, 14], physicalCores: 1, load: [1.5, 1.2, 1] },
	memory: { total: 32 * GB, used: 10 * GB, available: 22 * GB, swapTotal: 0, swapUsed: 0 },
	disks: [
		{ mount: "/", fileSystem: "ext4", total: 100 * GB, available: 50 * GB },
		{ mount: "/data", fileSystem: "ext4", total: 100 * GB, available: 10 * GB },
	],
	processCount: 4,
	processes: [
		process(1, "docker", 2),
		process(2, "docker", 1),
		process(3, "claude", 5, true),
		process(4, "ahab", 1, true),
	],
	apps: [
		{ name: "claude", processes: 1, cpuPercent: 5, memoryBytes: GB, members: [3] },
		{ name: "docker", processes: 2, cpuPercent: 3, memoryBytes: GB, members: [1, 2] },
		{ name: "ahab", processes: 1, cpuPercent: 1, memoryBytes: GB, members: [4] },
	],
	ahab: [
		{ kind: "agent", id: "claude-code", count: 2, processes: 9, cpuPercent: 5, memoryBytes: GB },
		{ kind: "plugin", id: "docker", count: 1, processes: 1, cpuPercent: 0.1, memoryBytes: GB },
		{ kind: "ahab", id: null, count: 1, processes: 3, cpuPercent: 1, memoryBytes: GB },
	],
	history: [
		{
			at: "2026-11-16T10:00:00Z",
			cpu: 10,
			memoryUsed: 9 * GB,
			swapUsed: 0,
			load: 1,
			ahabCpu: 4,
			ahabMemory: GB,
		},
		{
			at: "2026-11-16T10:00:03Z",
			cpu: 12,
			memoryUsed: 10 * GB,
			swapUsed: 0,
			load: 1.5,
			ahabCpu: 6,
			ahabMemory: GB,
		},
	],
};

describe("MachineResources", () => {
	it("answers at a glance, then says what Ahab uses the machine for", async () => {
		mockServer.use(
			getMachineMetricsMockHandler(metrics),
			getAgentCatalogMockHandler({
				items: [],
				registryVersion: "1.0.0",
				registrySource: "bundled",
			}),
		);
		renderWithRouter(<MachineResources machineId="m1" />);

		const glance = await screen.findByRole("list", { name: "At a glance" });
		expect(glance).toHaveTextContent("CPU12%");
		expect(glance).toHaveTextContent("Memory31%");
		expect(glance).toHaveTextContent("Disk /data90%");
		// A disk past 80% opens the disks first.
		expect(screen.getByRole("tab", { name: "Disks (2)" })).toHaveAttribute("aria-selected", "true");

		const ahab = screen.getByRole("heading", { name: "Ahab on this machine" }).closest("section");
		expect(ahab).toHaveTextContent("Claude Code2 sessions · 9 processes");
		expect(ahab).toHaveTextContent("Plugin: docker");
		expect(
			within(ahab as HTMLElement).getByRole("link", { name: "Open Claude Code" }),
		).toHaveAttribute("href", "/settings/machines/m1/agents?agent=claude-code");
	});

	it("lists processes by program, as a tree or one by one, and opens one's details", async () => {
		const user = userEvent.setup();
		mockServer.use(
			getMachineMetricsMockHandler(metrics),
			getAgentCatalogMockHandler({
				items: [],
				registryVersion: "1.0.0",
				registrySource: "bundled",
			}),
		);
		renderWithRouter(<MachineResources machineId="m1" />);

		await user.click(await screen.findByRole("tab", { name: "Processes (4)" }));
		const table = screen.getByRole("table");
		await user.click(within(table).getByRole("button", { name: /^docker/ }));
		expect(within(table).getByRole("button", { name: "docker 1: details" })).toBeInTheDocument();
		expect(within(table).getByRole("button", { name: "docker 2: details" })).toBeInTheDocument();

		await user.click(screen.getByRole("button", { name: "Ahab's" }));
		expect(within(table).queryByRole("button", { name: /^docker/ })).not.toBeInTheDocument();
		expect(within(table).getByRole("button", { name: "claude 3: details" })).toBeInTheDocument();

		await user.click(screen.getByRole("button", { name: "All" }));
		await user.click(screen.getByRole("button", { name: "One by one" }));
		await user.type(screen.getByRole("textbox", { name: "Search processes" }), "dock");
		expect(
			within(table).queryByRole("button", { name: "claude 3: details" }),
		).not.toBeInTheDocument();

		await user.click(within(table).getByRole("button", { name: "docker 1: details" }));
		const drawer = await screen.findByRole("dialog", { name: "docker" });
		expect(drawer).toHaveTextContent("/usr/bin/docker");
		expect(drawer).toHaveTextContent("alex");
		expect(drawer).toHaveTextContent("2.0 KB/s written");
	});

	it("says when the machine is offline", async () => {
		mockServer.use(
			http.get("*/api/v1/machines/m1/metrics", () =>
				HttpResponse.json(
					{ title: "Offline", status: 503, code: "machine_offline" },
					{ status: 503, headers: { "content-type": "application/problem+json" } },
				),
			),
		);
		renderWithRouter(<MachineResources machineId="m1" />);

		expect(await screen.findByText(/The machine is offline/)).toBeInTheDocument();
	});
});
