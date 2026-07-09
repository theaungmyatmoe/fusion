package tui

import (
	"bufio"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"

	"github.com/aungmyatmoe/zencode/internal/config"
	"github.com/aungmyatmoe/zencode/internal/ui"
	"github.com/aungmyatmoe/zencode/internal/util"
)

// SimpleREPL is the mobile / Termux-native "open" interface.
// It prints everything to normal terminal scrollback (finger scroll + copy work great in Termux).
// Favors slash commands over hard modifier keys. Very forgiving on small screens and soft keyboards.
type SimpleREPL struct {
	cfg   *config.Config
	yolo  bool
	mode  string // Normal | Plan | YOLO
	scan  *bufio.Scanner
	theme *lipgloss.Renderer // we just use the styles directly
}

func NewSimple(cfg *config.Config) *SimpleREPL {
	yolo := cfg.Yolo
	mode := "Normal"
	if yolo {
		mode = "YOLO"
	}
	return &SimpleREPL{
		cfg:  cfg,
		yolo: yolo,
		mode: mode,
		scan: bufio.NewScanner(os.Stdin),
	}
}

func (s *SimpleREPL) Run() error {
	s.printWelcome()

	for {
		fmt.Print(ui.PromptStyle.Render("z> ") + " ")
		if !s.scan.Scan() {
			break
		}
		line := strings.TrimSpace(s.scan.Text())
		if line == "" {
			continue
		}

		// Slash commands first (easiest on mobile soft keyboard)
		if strings.HasPrefix(line, "/") {
			if s.handleSlash(line) {
				continue
			}
			// unknown command falls through or handled
		}

		s.printUser(line)

		// Simulate the agent (real loop + tools will replace this)
		s.simulateAgent(line)

		// Small pause so output doesn't feel instant on real models later
		time.Sleep(80 * time.Millisecond)
	}

	fmt.Println(ui.Dim.Render("\nbye"))
	return nil
}

func (s *SimpleREPL) printWelcome() {
	termux := isTermux()
	kind := "terminal"
	if termux {
		kind = "Termux"
	}

	fmt.Println(ui.Accent.Render("zencode") + ui.Dim.Render("  mobile-first coding agent for "+kind))
	fmt.Printf("model: %s   mode: %s\n", s.cfg.Model, s.mode)
	if s.cfg.ConfigPath != "" {
		fmt.Printf(ui.Dim.Render("config: %s\n"), s.cfg.ConfigPath)
	}

	fmt.Println(ui.Dim.Render("Type /help for commands. Everything stays in your normal scrollback (great for phones)."))
	fmt.Println()
}

func (s *SimpleREPL) printUser(text string) {
	fmt.Println(ui.UserHeader.Render("You:") + " " + text)
}

func (s *SimpleREPL) simulateAgent(userMsg string) {
	lower := strings.ToLower(userMsg)

	// Fake "thinking" line
	fmt.Print(ui.Thinking.Render("Agent thinking..."))
	time.Sleep(90 * time.Millisecond)
	fmt.Print("\r" + strings.Repeat(" ", 30) + "\r") // clear the line

	if strings.Contains(lower, "todo") || strings.Contains(lower, "plan") {
		fmt.Println(ui.Success.Render("Agent:") + " I'll track this with todos (visible and persistent in real version).")
		fmt.Println(ui.Dim.Render("  [todo_write]"))
		fmt.Println("  - [ ] Understand the request")
		fmt.Println("  - [x] Show nice mobile output")
		fmt.Println("  - [ ] Implement real search_replace + run cmd")
		fmt.Println()
		return
	}

	if strings.Contains(lower, "edit") || strings.Contains(lower, "fix") || strings.Contains(lower, "change") {
		fmt.Println(ui.Success.Render("Agent:") + " Using the precise search_replace tool (must match exactly once).")
		fmt.Println(ui.Dim.Render("  (this is the important safety property — ambiguous replaces are rejected)"))

		// Actually exercise the real safety util we have
		demoContent := "package main\n\nfunc main() {\n\tfmt.Println(\"hello\")\n}\n"
		oldStr := "\tfmt.Println(\"hello\")"
		newStr := "\tfmt.Println(\"hello from zencode on Termux\")\n\t// TODO: more useful code"

		_, res, err := util.ApplySearchReplace(demoContent, oldStr, newStr)
		if err != nil {
			fmt.Println("  (demo replace would have been rejected: " + err.Error() + ")")
		} else {
			fmt.Println(ui.DiffHeader.Render("```diff"))
			fmt.Print(res.Diff)
			fmt.Println("```")
		}
		fmt.Println()

		if !s.yolo {
			fmt.Print("Apply this change? [y/N/a(lways)] ")
			if s.scan.Scan() {
				ans := strings.ToLower(strings.TrimSpace(s.scan.Text()))
				if ans == "y" || ans == "a" {
					fmt.Println(ui.Success.Render("  → change applied (in real run we would write the file)"))
					if ans == "a" {
						s.yolo = true
						s.mode = "YOLO"
						fmt.Println(ui.Warning.Render("  YOLO mode enabled for this session"))
					}
				} else {
					fmt.Println(ui.Dim.Render("  → skipped"))
				}
			}
		} else {
			fmt.Println(ui.Success.Render("  (YOLO: would auto-apply)"))
		}
		return
	}

	// Default response
	fmt.Println(ui.Success.Render("Agent:") + " Got it. (This is the lightweight open terminal interface — perfect for Termux on phones.)")
	fmt.Println(ui.Dim.Render("  Real model calls + full tool set (read/grep/edit/shell/todo/subagent) coming next."))
	fmt.Println(ui.Dim.Render("  Try messages with 'todo', 'plan', or 'edit'. Use /help for controls."))
	fmt.Println()
}

func (s *SimpleREPL) handleSlash(line string) bool {
	cmd := strings.ToLower(strings.TrimSpace(line))
	switch {
	case cmd == "/help" || cmd == "/h" || cmd == "/?":
		s.printHelp()
		return true

	case cmd == "/yolo" || cmd == "/always-approve":
		s.yolo = !s.yolo
		if s.yolo {
			s.mode = "YOLO"
			fmt.Println(ui.Warning.Render("YOLO mode ON — edits and commands will auto-approve"))
		} else {
			s.mode = "Normal"
			fmt.Println("YOLO mode OFF")
		}
		return true

	case cmd == "/plan":
		fmt.Println(ui.Accent.Render("Plan mode") + " (stub): the agent will explore/read but not edit until you approve a plan.")
		fmt.Println("In the full version this writes a reviewable plan.md and shows it for approval.")
		return true

	case cmd == "/model" || strings.HasPrefix(cmd, "/model "):
		parts := strings.SplitN(line, " ", 2)
		if len(parts) == 2 && strings.TrimSpace(parts[1]) != "" {
			s.cfg.Model = strings.TrimSpace(parts[1])
			fmt.Printf("Model switched to %s\n", s.cfg.Model)
		} else {
			fmt.Printf("Current model: %s\n", s.cfg.Model)
			fmt.Println("Usage: /model grok-3   or   /model claude-3-5-sonnet")
		}
		return true

	case cmd == "/exit" || cmd == "/quit" || cmd == "/q":
		fmt.Println("Exiting...")
		os.Exit(0)
		return true

	case cmd == "/status":
		fmt.Printf("model=%s  mode=%s  yolo=%v\n", s.cfg.Model, s.mode, s.yolo)
		return true

	default:
		fmt.Println(ui.Dim.Render("Unknown command. Try /help"))
		return true
	}
}

func (s *SimpleREPL) printHelp() {
	fmt.Println(ui.Accent.Render("Zencode commands (mobile friendly):"))
	fmt.Println("  /help, /h          this help")
	fmt.Println("  /yolo              toggle auto-approve (dangerous but fast on phone)")
	fmt.Println("  /plan              enter plan mode (design before code)")
	fmt.Println("  /model <name>      switch model (e.g. grok-3, gpt-4o)")
	fmt.Println("  /status            show current settings")
	fmt.Println("  /exit, /quit       leave")
	fmt.Println()
	fmt.Println(ui.Dim.Render("Just type normally to talk to the agent."))
	fmt.Println(ui.Dim.Render("On Termux: volume-down often acts as Ctrl. Slash commands are easiest."))
	fmt.Println()
}

func isTermux() bool {
	// Common Termux indicators
	if os.Getenv("TERMUX_VERSION") != "" {
		return true
	}
	prefix := os.Getenv("PREFIX")
	if strings.Contains(prefix, "com.termux") {
		return true
	}
	return false
}
