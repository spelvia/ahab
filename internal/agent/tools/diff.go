package tools

import (
	"fmt"
	"strings"
)

// unifiedDiff renders a minimal unified-style diff between two texts. It is
// LCS-based and intended for manifest-sized files shown in approval prompts.
func unifiedDiff(oldText, newText string) string {
	if oldText == newText {
		return "(no changes)"
	}
	oldLines := splitLines(oldText)
	newLines := splitLines(newText)

	// LCS table.
	m, n := len(oldLines), len(newLines)
	lcs := make([][]int, m+1)
	for i := range lcs {
		lcs[i] = make([]int, n+1)
	}
	for i := m - 1; i >= 0; i-- {
		for j := n - 1; j >= 0; j-- {
			if oldLines[i] == newLines[j] {
				lcs[i][j] = lcs[i+1][j+1] + 1
			} else {
				lcs[i][j] = max(lcs[i+1][j], lcs[i][j+1])
			}
		}
	}

	var b strings.Builder
	i, j := 0, 0
	for i < m && j < n {
		switch {
		case oldLines[i] == newLines[j]:
			fmt.Fprintf(&b, "  %s\n", oldLines[i])
			i++
			j++
		case lcs[i+1][j] >= lcs[i][j+1]:
			fmt.Fprintf(&b, "- %s\n", oldLines[i])
			i++
		default:
			fmt.Fprintf(&b, "+ %s\n", newLines[j])
			j++
		}
	}
	for ; i < m; i++ {
		fmt.Fprintf(&b, "- %s\n", oldLines[i])
	}
	for ; j < n; j++ {
		fmt.Fprintf(&b, "+ %s\n", newLines[j])
	}
	return strings.TrimSuffix(b.String(), "\n")
}

func splitLines(s string) []string {
	if s == "" {
		return nil
	}
	return strings.Split(strings.TrimSuffix(s, "\n"), "\n")
}
