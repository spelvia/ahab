package main

import (
	"errors"
	"strings"

	"github.com/spf13/cobra"
)

func newBuildCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "build [prompt]",
		Short: "Build mode: the agent plans, writes, and applies Kubernetes manifests under your review",
		Args:  cobra.ArbitraryArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig()
			if err != nil {
				return err
			}
			return runBuild(cmd.Context(), cfg, strings.Join(args, " "))
		},
	}
}

func newInvestigateCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "investigate <problem>",
		Short: "Investigation mode: the agent diagnoses a cluster problem and writes a report",
		Args:  cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig()
			if err != nil {
				return err
			}
			return runInvestigate(cmd.Context(), cfg, strings.Join(args, " "))
		},
	}
}

func newHistoryCmd() *cobra.Command {
	var session string
	cmd := &cobra.Command{
		Use:   "history",
		Short: "Show the recorded command history tree for a session",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig()
			if err != nil {
				return err
			}
			return runHistory(cfg, session)
		},
	}
	cmd.Flags().StringVar(&session, "session", "", "session ID (defaults to the most recent)")
	return cmd
}

var errNotImplemented = errors.New("not implemented yet")
