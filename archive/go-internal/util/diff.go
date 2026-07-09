package util

import (
	"fmt"
	"strings"
)

// SearchReplaceResult is returned by ApplySearchReplace.
type SearchReplaceResult struct {
	OldText string
	NewText string
	Diff    string // simple unified-ish diff for display
	Applied bool
}

// ApplySearchReplace performs a precise, unique-match replacement.
// It mirrors the safety properties of high-quality agents: old_string must appear
// exactly once in the file content.
func ApplySearchReplace(content, oldString, newString string) (string, SearchReplaceResult, error) {
	if oldString == "" {
		return content, SearchReplaceResult{}, fmt.Errorf("old_string must not be empty")
	}

	count := strings.Count(content, oldString)
	if count == 0 {
		return content, SearchReplaceResult{}, fmt.Errorf("old_string not found in file")
	}
	if count > 1 {
		return content, SearchReplaceResult{}, fmt.Errorf("old_string is not unique (appears %d times) — make it longer/more specific", count)
	}

	// Perform replacement
	newContent := strings.Replace(content, oldString, newString, 1)

	res := SearchReplaceResult{
		OldText: oldString,
		NewText: newString,
		Applied: true,
		Diff:    makeSimpleDiff(oldString, newString),
	}
	return newContent, res, nil
}

// makeSimpleDiff produces a compact diff for the TUI transcript.
// It is intentionally small — real unified diffs can be generated later with more context.
func makeSimpleDiff(old, new string) string {
	var b strings.Builder
	b.WriteString("```diff\n")
	linesOld := strings.Split(old, "\n")
	linesNew := strings.Split(new, "\n")

	// Very naive but useful for the transcript
	max := len(linesOld)
	if len(linesNew) > max {
		max = len(linesNew)
	}
	for i := 0; i < max; i++ {
		o := ""
		n := ""
		if i < len(linesOld) {
			o = linesOld[i]
		}
		if i < len(linesNew) {
			n = linesNew[i]
		}
		if o == n {
			b.WriteString(" ")
			b.WriteString(o)
		} else {
			if o != "" {
				b.WriteString("-")
				b.WriteString(o)
				b.WriteString("\n")
			}
			if n != "" || o == "" {
				b.WriteString("+")
				b.WriteString(n)
			}
		}
		b.WriteString("\n")
	}
	b.WriteString("```")
	return b.String()
}

// FileTree returns a very simple textual tree (placeholder until we have better fs walking).
func FileTree(root string, maxDepth int) string {
	// Real implementation will walk respecting .gitignore and return a nice tree.
	// For scaffold we just note the root.
	return fmt.Sprintf("(project tree for %s — depth %d — real walker coming)", root, maxDepth)
}
