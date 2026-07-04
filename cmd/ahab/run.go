package main

import (
	"context"

	"github.com/spelvia/ahab/internal/config"
)

// These entry points are wired to the agent and TUI in later milestones.

func runBuild(ctx context.Context, cfg *config.Config, prompt string) error {
	return errNotImplemented
}

func runInvestigate(ctx context.Context, cfg *config.Config, problem string) error {
	return errNotImplemented
}

func runHistory(cfg *config.Config, session string) error {
	return errNotImplemented
}
