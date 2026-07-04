// Command ahab is an agentic harness for building and maintaining Kubernetes
// systems: an LLM agent plans, writes, and applies manifests and investigates
// cluster problems while the operator supervises every consequential step.
package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"

	"github.com/spelvia/ahab/internal/config"
)

var (
	flagAuto        bool
	flagKubeContext string
)

func main() {
	if err := newRootCmd().Execute(); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

func newRootCmd() *cobra.Command {
	root := &cobra.Command{
		Use:   "ahab",
		Short: "Agentic harness for Kubernetes systems",
		Long: "ahab drives an LLM agent that builds and maintains Kubernetes systems.\n" +
			"You supervise: the agent proposes plans, file changes, and commands,\n" +
			"and nothing consequential runs without your approval.",
		SilenceUsage:  true,
		SilenceErrors: true,
	}
	root.PersistentFlags().BoolVar(&flagAuto, "auto", false, "run without approval gates (not yet supported)")
	root.PersistentFlags().StringVar(&flagKubeContext, "context", "", "kubeconfig context to use")

	root.AddCommand(newBuildCmd(), newInvestigateCmd(), newHistoryCmd())
	return root
}

func loadConfig() (*config.Config, error) {
	wd, err := os.Getwd()
	if err != nil {
		return nil, err
	}
	cfg, err := config.Load(wd)
	if err != nil {
		return nil, err
	}
	if flagKubeContext != "" {
		cfg.KubeContext = flagKubeContext
	}
	if flagAuto {
		return nil, fmt.Errorf("--auto is not yet supported: v1 always runs with approval gates")
	}
	return cfg, nil
}
