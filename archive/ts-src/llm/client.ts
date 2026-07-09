import { OpenAI } from "openai";
import type { Config } from "../config.js";

export interface ChatMessage {
  role: "system" | "user" | "assistant" | "tool";
  content: string;
  name?: string;
}

export interface ChatCompletionOptions {
  messages: ChatMessage[];
  tools?: any[];
  temperature?: number;
  max_tokens?: number;
}

export interface ChatCompletionResult {
  content: string;
  reasoning_content?: string;
  tool_calls?: any[];
  raw?: any;
}

/**
 * Flexible LLM client supporting:
 * - Cloudflare Workers AI (default for open models like Kimi, GLM)
 * - xAI / Grok (OpenAI compatible)
 * - Any OpenAI-compatible provider (Groq, Together, OpenRouter, Ollama, local, etc.)
 *
 * This is how OpenCode and similar agents do multi-provider routing.
 */
export class LLMClient {
  private config: Config;
  private openai?: OpenAI;

  constructor(config: Config) {
    this.config = config;

    // For standard OpenAI-compatible providers (xAI, Groq, Together, etc.)
    if (config.provider !== "cloudflare" && config.baseURL) {
      this.openai = new OpenAI({
        apiKey: config.apiKey || "dummy",
        baseURL: config.baseURL,
      });
    }
  }

  async chat(options: ChatCompletionOptions): Promise<ChatCompletionResult> {
    const { model, provider } = this.config;

    if (provider === "cloudflare" || model.startsWith("@cf/")) {
      return this.chatCloudflare(options);
    }

    if (this.openai) {
      return this.chatOpenAICompatible(options);
    }

    // Fallback: try to treat as OpenAI compatible
    if (this.config.baseURL) {
      const openai = new OpenAI({
        apiKey: this.config.apiKey || "dummy",
        baseURL: this.config.baseURL,
      });
      return this.chatWithClient(openai, options);
    }

    throw new Error("No valid LLM provider configured. Set ZENCODE_PROVIDER or provide API keys.");
  }

  private async chatCloudflare(options: ChatCompletionOptions): Promise<ChatCompletionResult> {
    const { cloudflareAccountId, apiKey, model } = this.config;

    if (!cloudflareAccountId) {
      const cfgPath = (this.config as any)?.configPath;
      throw new Error(
        "CLOUDFLARE_ACCOUNT_ID is required when using Cloudflare Workers AI\n" +
        (cfgPath ? `  (config file examined: ${cfgPath})\n` : "") +
        "Tip: export CLOUDFLARE_ACCOUNT_ID + CLOUDFLARE_API_TOKEN (or put them in ~/.config/zencode/zencode.json under provider.cloudflare.options) and restart your shell. " +
        "Env vars take highest priority."
      );
    }
    if (!apiKey) {
      const cfgPath = (this.config as any)?.configPath;
      throw new Error(
        "CLOUDFLARE_API_TOKEN (or ZENCODE_API_KEY) is required for Cloudflare\n" +
        (cfgPath ? `  (config file examined: ${cfgPath})\n` : "") +
        "Tip: export CLOUDFLARE_ACCOUNT_ID + CLOUDFLARE_API_TOKEN (or put them in ~/.config/zencode/zencode.json under provider.cloudflare.options) and restart your shell."
      );
    }

    const url = `https://api.cloudflare.com/client/v4/accounts/${cloudflareAccountId}/ai/run/${model}`;

    const body: any = {
      messages: options.messages,
      temperature: options.temperature ?? 0.6,
    };

    if (options.max_tokens) body.max_tokens = options.max_tokens;
    if (options.tools) body.tools = options.tools;

    const res = await fetch(url, {
      method: "POST",
      headers: {
        "Authorization": `Bearer ${apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
    });

    if (!res.ok) {
      const text = await res.text();
      throw new Error(`Cloudflare AI error (${res.status}): ${text}`);
    }

    const data = await res.json() as any;

    // Workers AI now returns OpenAI-compatible format for many models (including Kimi K2.7)
    // Fallbacks for older response shapes.
    let content =
      data?.result?.choices?.[0]?.message?.content ??
      data?.result?.response ??
      data?.result?.text ??
      (typeof data?.result === "string" ? data.result : "") ??
      "";

    return {
      content: typeof content === "string" ? content : JSON.stringify(content),
      reasoning_content: data?.result?.choices?.[0]?.message?.reasoning_content || data?.result?.choices?.[0]?.message?.reasoning,
      tool_calls: data?.result?.choices?.[0]?.message?.tool_calls,
      raw: data,
    };
  }

  private async chatOpenAICompatible(options: ChatCompletionOptions): Promise<ChatCompletionResult> {
    if (!this.openai) throw new Error("OpenAI client not initialized");
    return this.chatWithClient(this.openai, options);
  }

  private async chatWithClient(client: OpenAI, options: ChatCompletionOptions): Promise<ChatCompletionResult> {
    const completion = await client.chat.completions.create({
      model: this.config.model,
      messages: options.messages as any,
      tools: options.tools as any,
      temperature: options.temperature,
      max_tokens: options.max_tokens,
    });

    const choice = completion.choices[0];
    return {
      content: choice.message.content || "",
      reasoning_content: (choice.message as any).reasoning_content || (choice.message as any).reasoning,
      tool_calls: choice.message.tool_calls,
      raw: completion,
    };
  }
}

export function createLLMClient(config: Config): LLMClient {
  return new LLMClient(config);
}
