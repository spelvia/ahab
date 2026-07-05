package main

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/spelvia/ahab/internal/agent"
	"github.com/spelvia/ahab/internal/agent/build"
	"github.com/spelvia/ahab/internal/agent/investigate"
	"github.com/spelvia/ahab/internal/config"
	"github.com/spelvia/ahab/internal/executor"
	"github.com/spelvia/ahab/internal/llm/factory"
	"github.com/spelvia/ahab/internal/recorder"
	"github.com/spelvia/ahab/internal/tui"
)

func runBuild(ctx context.Context, cfg *config.Config, prompt string) error {
	if strings.TrimSpace(prompt) == "" {
		return errors.New(`usage: ahab build "<what to build or change>"`)
	}
	provider, err := factory.New(cfg)
	if err != nil {
		return err
	}
	runner := executor.New(cfg.KubeContext, cfg.Namespace)
	sessionID := agent.NewSessionID()

	dir, err := cfg.HistoryDir()
	if err != nil {
		return err
	}
	rec, err := recorder.New(dir, sessionID, "build", prompt)
	if err != nil {
		return err
	}
	defer rec.Close()

	var loop *agent.Loop
	return tui.Run(ctx, tui.Options{
		Title:         "ahab · build",
		Mode:          "build",
		SessionID:     sessionID,
		InitialPrompt: prompt,
		FollowUp:      true,
		Recorder:      rec,
		Turn: func(ctx context.Context, s tui.Session, input string) error {
			if loop == nil {
				loop, err = build.New(build.Deps{
					Provider:  provider,
					Runner:    runner,
					UI:        s.UI,
					Recorder:  s.Recorder,
					Gate:      s.Gate,
					Ask:       s.Ask,
					WorkDir:   cfg.WorkDir,
					Repos:     cfg.Repos,
					MaxTokens: cfg.MaxTokens,
				})
				if err != nil {
					return err
				}
			}
			return loop.Run(ctx, input)
		},
	})
}

func runInvestigate(ctx context.Context, cfg *config.Config, problem string) error {
	provider, err := factory.New(cfg)
	if err != nil {
		return err
	}
	runner := executor.New(cfg.KubeContext, cfg.Namespace)
	sessionID := agent.NewSessionID()

	historyDir, err := cfg.HistoryDir()
	if err != nil {
		return err
	}
	reportsDir, err := cfg.ReportsDir()
	if err != nil {
		return err
	}
	rec, err := recorder.New(historyDir, sessionID, "investigate", problem)
	if err != nil {
		return err
	}
	defer rec.Close()

	var reportPath string
	err = tui.Run(ctx, tui.Options{
		Title:         "ahab · investigate",
		Mode:          "investigate",
		SessionID:     sessionID,
		InitialPrompt: problem,
		Recorder:      rec,
		Turn: func(ctx context.Context, s tui.Session, input string) error {
			reportPath, err = investigate.Run(ctx, investigate.Deps{
				Provider:   provider,
				Runner:     runner,
				UI:         s.UI,
				Recorder:   s.Recorder,
				Ask:        s.Ask,
				WorkDir:    cfg.WorkDir,
				Repos:      cfg.Repos,
				ReportsDir: reportsDir,
				SessionID:  sessionID,
				MaxTokens:  cfg.MaxTokens,
			}, input)
			return err
		},
	})
	if reportPath != "" {
		fmt.Println("report:", reportPath)
	}
	return err
}

func runHistory(cfg *config.Config, session string) error {
	dir, err := cfg.HistoryDir()
	if err != nil {
		return err
	}
	if session == "" {
		session, err = recorder.Latest(dir)
		if err != nil {
			return err
		}
		if session == "" {
			fmt.Println("no sessions recorded yet")
			return nil
		}
	}
	s, err := recorder.Load(dir, session)
	if err != nil {
		return err
	}
	fmt.Print(recorder.RenderTree(s))

	if ids, err := recorder.List(dir); err == nil && len(ids) > 1 {
		fmt.Printf("\n%d sessions available: %s\n", len(ids), strings.Join(ids, ", "))
	}
	return nil
}
