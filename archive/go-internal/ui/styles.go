package ui

import "github.com/charmbracelet/lipgloss"

// Colors and base styles for the TUI. Inspired by modern terminal agents.
var (
	// Base
	Normal     = lipgloss.NewStyle()
	Dim        = lipgloss.NewStyle().Foreground(lipgloss.Color("241"))
	Accent     = lipgloss.NewStyle().Foreground(lipgloss.Color("63")) // purple-ish
	Success    = lipgloss.NewStyle().Foreground(lipgloss.Color("42"))
	Warning    = lipgloss.NewStyle().Foreground(lipgloss.Color("214"))
	ErrorStyle = lipgloss.NewStyle().Foreground(lipgloss.Color("196"))

	// Blocks
	UserHeader   = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("39"))
	Assistant    = lipgloss.NewStyle().Foreground(lipgloss.Color("252"))
	ToolHeader   = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("208"))
	Thinking     = lipgloss.NewStyle().Foreground(lipgloss.Color("245")).Italic(true)

	// Diff / edit
	DiffAdd    = lipgloss.NewStyle().Foreground(lipgloss.Color("42"))
	DiffDel    = lipgloss.NewStyle().Foreground(lipgloss.Color("196"))
	DiffHeader = lipgloss.NewStyle().Foreground(lipgloss.Color("63")).Bold(true)

	// Status / bars
	StatusBar = lipgloss.NewStyle().
			Foreground(lipgloss.Color("252")).
			Background(lipgloss.Color("236")).
			Padding(0, 1)

	// Boxes
	Box = lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("63")).
		Padding(0, 1)

	// Prompt
	PromptStyle = lipgloss.NewStyle().Foreground(lipgloss.Color("63")).Bold(true)
)
