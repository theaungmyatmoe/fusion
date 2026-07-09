package llm

import (
	"context"
	"fmt"
	"io"

	openai "github.com/sashabaranov/go-openai"
)

// Client is a thin wrapper around the OpenAI-compatible client.
// It is the single place we talk to Grok (xAI), OpenAI, Groq, local servers, etc.
type Client struct {
	inner   *openai.Client
	model   string
	baseURL string
}

// New creates a client configured for the given base and key.
// For Grok: baseURL = "https://api.x.ai/v1", key = XAI_API_KEY
func New(baseURL, apiKey, model string) *Client {
	config := openai.DefaultConfig(apiKey)
	if baseURL != "" {
		config.BaseURL = baseURL
	}
	return &Client{
		inner:   openai.NewClientWithConfig(config),
		model:   model,
		baseURL: baseURL,
	}
}

// CompletionRequest is our simplified request shape (we'll expand with tools etc).
type CompletionRequest struct {
	Messages []openai.ChatCompletionMessage
	Tools    []openai.Tool
}

// StreamCompletion streams assistant content + tool call deltas.
// This is the heart of the interactive "Grok-like" experience.
func (c *Client) StreamCompletion(ctx context.Context, req CompletionRequest) (<-chan string, error) {
	// TODO: implement real streaming + tool call accumulation using
	// c.inner.CreateChatCompletionStream + openai.ChatCompletionRequest
	// For now this is a stub so the rest of the codebase can be built against the interface.

	ch := make(chan string, 8)
	go func() {
		defer close(ch)
		ch <- fmt.Sprintf("[llm stub] would stream from %s model=%s (tools=%d)\n", c.baseURL, c.model, len(req.Tools))
		ch <- "The real implementation will use go-openai streaming and feed deltas into the TUI + agent loop.\n"
	}()
	return ch, nil
}

// Simple non-stream helper (useful for subagents, compaction, plan summarization, etc.)
func (c *Client) Complete(ctx context.Context, req CompletionRequest) (string, error) {
	stream, err := c.StreamCompletion(ctx, req)
	if err != nil {
		return "", err
	}
	var out string
	for chunk := range stream {
		out += chunk
	}
	return out, nil
}

// Example of how we will register tools for the model (called from agent).
func (c *Client) BuildToolSchemas(tools []any) []openai.Tool {
	// In the real version we convert our Tool interface into []openai.Tool
	// using the function calling / tools schema.
	return nil
}

// Helper to create a user message.
func UserMessage(content string) openai.ChatCompletionMessage {
	return openai.ChatCompletionMessage{
		Role:    openai.ChatMessageRoleUser,
		Content: content,
	}
}

// Helper to create a system message (rules + instructions + current mode + tool list will go here).
func SystemMessage(content string) openai.ChatCompletionMessage {
	return openai.ChatCompletionMessage{
		Role:    openai.ChatMessageRoleSystem,
		Content: content,
	}
}

// Drain is a tiny helper in case we want to consume a channel without using it.
func Drain(ch <-chan string) {
	for range ch {
		// discard (used during tests or background)
	}
}

// Write an io.Writer adapter if we ever want to tee the stream.
type streamWriter struct {
	ch chan<- string
}

func (w streamWriter) Write(p []byte) (int, error) {
	w.ch <- string(p)
	return len(p), nil
}

var _ io.Writer = (*streamWriter)(nil)
