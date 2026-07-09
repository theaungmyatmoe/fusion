package agent

import (
	"context"
	"fmt"

	"github.com/aungmyatmoe/zencode/internal/util"
)

// Tool is the interface all agent capabilities implement.
// This will later be turned into OpenAI tool schemas + dispatch.
type Tool interface {
	Name() string
	Description() string
	// Schema() will return the openai.Tool definition
	Execute(ctx context.Context, input map[string]any) (string, error)
}

// ReadFileTool is a placeholder for the real implementation.
type ReadFileTool struct{}

func (ReadFileTool) Name() string        { return "read_file" }
func (ReadFileTool) Description() string { return "Read file contents (with optional line range)" }
func (ReadFileTool) Execute(ctx context.Context, input map[string]any) (string, error) {
	// Real version will do io + permission checks (read is usually auto-approved)
	path, _ := input["path"].(string)
	return fmt.Sprintf("[read_file stub] would read %s", path), nil
}

// SearchReplaceTool is the critical precise-edit tool (Grok-like safety).
type SearchReplaceTool struct{}

func (SearchReplaceTool) Name() string        { return "search_replace" }
func (SearchReplaceTool) Description() string { return "Precise string replacement. old_string must appear exactly once." }
func (SearchReplaceTool) Execute(ctx context.Context, input map[string]any) (string, error) {
	path, _ := input["path"].(string)
	oldS, _ := input["old_string"].(string)
	newS, _ := input["new_string"].(string)

	// In real agent we would:
	// 1. Read current content (respecting permissions)
	// 2. Call util.ApplySearchReplace
	// 3. Write back if success
	// 4. Return diff for transcript

	_, res, err := util.ApplySearchReplace("CURRENT FILE CONTENT WOULD BE HERE", oldS, newS)
	if err != nil {
		return "", err
	}
	return fmt.Sprintf("search_replace on %s succeeded\n%s", path, res.Diff), nil
}

// TodoWriteTool updates the visible task list (auto-approved, very Grok-like).
type TodoWriteTool struct{}

func (TodoWriteTool) Name() string        { return "todo_write" }
func (TodoWriteTool) Description() string { return "Create or update the visible task list for complex work" }
func (TodoWriteTool) Execute(ctx context.Context, input map[string]any) (string, error) {
	// Real impl will mutate session todos and render snapshots in the TUI
	return "[todo_write] tasks updated (visible in todos pane)", nil
}

// The real set of tools will also include: list_dir, grep (rg preferred), run_terminal_command (with safety),
// spawn_subagent, web_search, etc. They will be registered and exposed to the LLM via tool schemas.
