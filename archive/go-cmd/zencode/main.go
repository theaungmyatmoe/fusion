package main

import (
	"flag"
	"fmt"
	"os"
	"runtime"
	"strings"

	"github.com/aungmyatmoe/zencode/internal/config"
	"github.com/aungmyatmoe/zencode/internal/tui"
)

const (
	version = "0.1.0-dev"
	name    = "zencode"
)

func main() {
	var (
		showVersion = flag.Bool("version", false, "Print version and exit")
		showHelp    = flag.Bool("help", false, "Show help")
		cwd         = flag.String("cwd", "", "Working directory (defaults to current)")
		model       = flag.String("model", "", "Model to use (overrides config / XAI_API_KEY default)")
		yolo        = flag.Bool("yolo", false, "Auto-approve all tool actions (use with care)")
		prompt      = flag.String("p", "", "Headless prompt (non-interactive run)")
		outputFmt   = flag.String("output", "plain", "Headless output format: plain|json")
		useTUI      = flag.Bool("tui", false, "Use the richer bubbletea TUI (default is the lightweight open terminal REPL — better on phones)")
		useSimple   = flag.Bool("simple", false, "Force the simple mobile-friendly terminal REPL")
	)

	flag.Usage = func() {
		fmt.Fprintf(os.Stderr, "%s — AI coding agent for Termux (and terminals)\n\n", name)
		fmt.Fprintf(os.Stderr, "Usage:\n")
		fmt.Fprintf(os.Stderr, "  %s [flags] [initial prompt...]\n\n", name)
		fmt.Fprintf(os.Stderr, "Flags:\n")
		flag.PrintDefaults()
		fmt.Fprintf(os.Stderr, "\nEnvironment:\n")
		fmt.Fprintf(os.Stderr, "  XAI_API_KEY     Grok / xAI API key (recommended starting point)\n")
		fmt.Fprintf(os.Stderr, "  ZENCODE_API_KEY Fallback / generic key\n")
		fmt.Fprintf(os.Stderr, "  ZENCODE_BASE_URL Custom OpenAI-compatible base (e.g. for Ollama or proxy)\n")
		fmt.Fprintf(os.Stderr, "  ZENCODE_MODEL   Default model id\n\n")
		fmt.Fprintf(os.Stderr, "Examples:\n")
		fmt.Fprintf(os.Stderr, "  export XAI_API_KEY=...\n")
		fmt.Fprintf(os.Stderr, "  %s\n", name)
		fmt.Fprintf(os.Stderr, "  %s -p \"Add a README section about Termux setup\" --yolo\n", name)
		fmt.Fprintf(os.Stderr, "  %s --cwd ~/projects/myapp\n\n", name)
		fmt.Fprintf(os.Stderr, "Default interface is the lightweight open terminal REPL (best for phones in Termux).\n")
		fmt.Fprintf(os.Stderr, "Use --tui for the richer bubbletea TUI.\n")
	}

	flag.Parse()

	if *showVersion {
		fmt.Printf("%s %s (%s/%s)\n", name, version, runtime.GOOS, runtime.GOARCH)
		return
	}
	if *showHelp {
		flag.Usage()
		return
	}

	// Remaining args after flags can form an initial prompt (headless friendly)
	args := flag.Args()
	if *prompt == "" && len(args) > 0 {
		*prompt = strings.Join(args, " ")
	}

	if *cwd != "" {
		if err := os.Chdir(*cwd); err != nil {
			fmt.Fprintf(os.Stderr, "error: chdir %s: %v\n", *cwd, err)
			os.Exit(1)
		}
	}

	// Load configuration (env + file). CLI flags applied on top where relevant.
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintf(os.Stderr, "config warning: %v\n", err)
	}
	if *model != "" {
		cfg.Model = *model
	}
	if *yolo {
		cfg.Yolo = true
	}

	if *prompt != "" {
		// Headless path (stub for now — real agent loop will use cfg + prompt)
		fmt.Printf("Zencode headless (stub): prompt=%q model=%q yolo=%v output=%s\n",
			*prompt, cfg.Model, cfg.Yolo, *outputFmt)
		fmt.Println("Full agent loop + tools coming in the next implementation steps.")
		fmt.Println("Set XAI_API_KEY (or ZENCODE_*) and run the TUI for the real experience once built.")
		return
	}

	// Decide interface.
	// Default (and strongly recommended on phones) = simple "open" terminal REPL.
	//   - output stays in normal scrollback → native Termux scrolling + copy work perfectly
	//   - slash commands are the primary control surface (much easier on soft keyboards)
	//   - compact, narrow-screen friendly
	//
	// Use --tui when you want the richer visual bubbletea experience (good on tablets or with external keyboard).
	termux := isTermux()
	wantTUI := *useTUI

	if wantTUI {
		fmt.Printf("%s %s — launching rich TUI (--tui)\n", name, version)
		if err := tui.Run(cfg); err != nil {
			fmt.Fprintf(os.Stderr, "tui error: %v\n", err)
			os.Exit(1)
		}
		return
	}

	// The Termux-native open interface most people on phones will want.
	fmt.Printf("%s %s — simple open terminal mode (mobile friendly)\n", name, version)
	if termux {
		fmt.Println("(detected Termux — using the interface optimized for phones)")
	}

	simple := tui.NewSimple(cfg)
	if err := simple.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
}

// isTermux detects Termux environment so we can give better mobile defaults.
func isTermux() bool {
	if os.Getenv("TERMUX_VERSION") != "" {
		return true
	}
	if strings.Contains(os.Getenv("PREFIX"), "com.termux") {
		return true
	}
	return false
}
