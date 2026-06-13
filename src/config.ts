import { homedir } from "node:os";
import { join, dirname, parse as pathParse } from "node:path";
import { existsSync, readFileSync } from "node:fs";

export type Provider = 'cloudflare' | 'xai' | 'openai' | 'auto';

export interface Config {
  provider: Provider;
  model: string;
  smallModel?: string;
  apiKey: string;                    // generic / xai / cloudflare token
  baseURL: string;
  cloudflareAccountId?: string;      // for Workers AI
  yolo: boolean;
  configPath?: string;
  settings?: Record<string, any>;    // extra settings from zencode.json "settings" section
  raw?: any;                         // full parsed zencode.json for advanced use
}

/**
 * Default routing (as requested):
 * - Provider: cloudflare (Workers AI)
 * - Model: @cf/moonshotai/kimi-k2.7-code  (Kimi 2.7 Code)
 *
 * This allows using strong open-source / hosted models (Kimi, GLM, etc.)
 * without needing an xAI key by default. User can still override with
 * XAI_API_KEY or explicit config to use Grok models.
 *
 * Similar to how OpenCode / many agents support multiple backends + free tiers.
 */
export function loadConfig(cwd: string = process.cwd()): Config {
  const home = homedir();

  // 1. Load global zencode.json (lowest file priority)
  const globalPath = join(home, ".config", "zencode", "zencode.json");
  let globalRaw: any = {};
  if (existsSync(globalPath)) {
    try {
      globalRaw = JSON.parse(readFileSync(globalPath, "utf8"));
    } catch {}
  }

  // 2. Walk up from cwd to find project-level zencode.json (higher priority than global)
  let projectPath: string | undefined;
  let projectRaw: any = {};
  let dir = cwd;
  const root = pathParse(dir).root;

  while (dir !== root) {
    const candidate = join(dir, "zencode.json");
    if (existsSync(candidate)) {
      projectPath = candidate;
      try {
        projectRaw = JSON.parse(readFileSync(candidate, "utf8"));
      } catch {}
      break;
    }
    // also support .zencode/zencode.json in project for cleanliness
    const dotCandidate = join(dir, ".zencode", "zencode.json");
    if (existsSync(dotCandidate)) {
      projectPath = dotCandidate;
      try {
        projectRaw = JSON.parse(readFileSync(dotCandidate, "utf8"));
      } catch {}
      break;
    }
    dir = dirname(dir);
  }

  // Merge file configs: global < project
  const fileRaw = { ...globalRaw, ...projectRaw };

  // 3. Env vars (highest priority, can override file)
  // User-provided defaults for immediate Kimi 2.7 usage (replace or override via env/zencode.json for production)
  const xaiKey = process.env.XAI_API_KEY || "";
  let cfToken = process.env.CLOUDFLARE_API_TOKEN || process.env.CLOUDFLARE_AI_TOKEN || "";
  let cfAccountId = process.env.CLOUDFLARE_ACCOUNT_ID || "";
  const genericKey = process.env.ZENCODE_API_KEY || xaiKey || cfToken;

  let provider: Provider = (process.env.ZENCODE_PROVIDER as Provider) || fileRaw.provider?.default || 'auto';
  let model = process.env.ZENCODE_MODEL || fileRaw.model || "@cf/moonshotai/kimi-k2.7-code";
  let smallModel = process.env.ZENCODE_SMALL_MODEL || fileRaw.small_model || fileRaw.smallModel;
  let base = process.env.ZENCODE_BASE_URL || "";

  // Extract from provider section (similar to opencode.json schema)
  const providers = fileRaw.provider || {};
  if (providers.cloudflare?.options) {
    const opts = providers.cloudflare.options;
    if (opts.accountId && !cfAccountId) cfAccountId = opts.accountId;
    if (opts.apiKey && !cfToken) cfToken = opts.apiKey;
    if (opts.baseURL && !base) base = opts.baseURL;
  }
  if (providers.xai?.options) {
    const opts = providers.xai.options;
    if (opts.apiKey && !xaiKey) {
      // will use in generic
    }
    if (opts.baseURL && !base) base = opts.baseURL;
  }

  // Also harvest common top-level / alternate locations for CF creds (forgiving for global ~/.config/zencode/zencode.json)
  // Supports:
  //   { "cloudflareAccountId": "...", "apiKey": "..." }
  //   { "accountId": "...", "cloudflareApiToken": "..." }
  //   { "cloudflare": { "accountId": "...", "apiKey": "..." } }
  //   { "provider": { "cloudflare": { "accountId": "...", "apiKey": "..." } } }  (non-.options)
  if (!cfAccountId) {
    cfAccountId =
      fileRaw.cloudflareAccountId ||
      fileRaw.accountId ||
      fileRaw.cloudflare?.accountId ||
      providers.cloudflare?.accountId ||
      "";
  }
  if (!cfToken) {
    cfToken =
      fileRaw.apiKey ||
      fileRaw.cloudflareApiToken ||
      fileRaw.cloudflare?.apiKey ||
      providers.cloudflare?.apiKey ||
      "";
  }

  // Auto-detect sensible default (respect file/env)
  if (provider === 'auto') {
    if (cfAccountId && (cfToken || genericKey)) {
      provider = 'cloudflare';
    } else if (xaiKey) {
      provider = 'xai';
    } else if (fileRaw.provider && Object.keys(fileRaw.provider).length > 0) {
      // pick first defined provider from file
      provider = Object.keys(fileRaw.provider)[0] as Provider;
    } else {
      provider = 'cloudflare';
    }
  }

  if (provider === 'cloudflare') {
    model = process.env.ZENCODE_MODEL || fileRaw.model || "@cf/moonshotai/kimi-k2.7-code";
  }

  // Normalize common opencode-style "provider/model" or "cloudflare/short" to full Workers AI id
  // Supports "cloudflare/kimi-k2.7-code" (user-friendly shorthand in zencode.json) → "@cf/moonshotai/kimi-k2.7-code"
  if (model.startsWith('cloudflare/')) {
    let rest = model.slice('cloudflare/'.length);
    if (!rest.includes('/')) {
      const shorthand: Record<string, string> = {
        'kimi-k2.7-code': 'moonshotai/kimi-k2.7-code',
        'kimi-k2.7': 'moonshotai/kimi-k2.7-code',
        'kimi': 'moonshotai/kimi-k2.7-code',
        'glm-4': 'zhipu-ai/glm-4',
        'glm-4-9b': 'zhipu-ai/glm-4',
        'qwen2.5-coder': 'qwen/qwen2.5-coder-32b-instruct',
      };
      rest = shorthand[rest] || rest;
    }
    model = '@cf/' + rest;
  }

  if (provider === 'xai' && !base) {
    base = "https://api.x.ai/v1";
  }

  // Legacy fallback for old config.json (only if no zencode.json was found at all)
  const hasGlobalZencode = globalRaw && Object.keys(globalRaw).length > 0;
  if (!projectPath && !hasGlobalZencode) {
    const legacyPaths = [
      join(home, ".zencode", "config.json"),
      join(home, ".config", "zencode", "config.json"),
    ];
    for (const p of legacyPaths) {
      if (existsSync(p)) {
        try {
          const raw = JSON.parse(readFileSync(p, "utf8"));
          if (!process.env.ZENCODE_MODEL && raw.model) model = raw.model;
          if (!process.env.ZENCODE_PROVIDER && raw.provider) provider = raw.provider;
          if (raw.cloudflareAccountId) cfAccountId = raw.cloudflareAccountId; // local var
        } catch {}
        break;
      }
    }
  }

  // Final baseURL
  if (!base) {
    if (provider === 'cloudflare') {
      base = "";
    } else {
      base = "https://api.openai.com/v1";
    }
  }

  // Settings section (opencode-style)
  const settings = {
    ...globalRaw.settings,
    ...projectRaw.settings,
  };

  const finalApiKey = genericKey;
  const finalCfAccount = cfAccountId || providers.cloudflare?.options?.accountId;
  const finalCfToken = cfToken || providers.cloudflare?.options?.apiKey;

  const result: Config = {
    provider,
    model,
    smallModel,
    apiKey: finalApiKey,
    baseURL: base,
    cloudflareAccountId: finalCfAccount,
    yolo: process.env.ZENCODE_YOLO === "1" || process.env.ZENCODE_YOLO === "true" || !!settings.yolo,
    configPath: projectPath || globalPath,
    settings,
    raw: fileRaw,
  };

  // If cloudflare token came from provider section, prefer it
  if (finalCfToken && !result.apiKey) {
    result.apiKey = finalCfToken;
  }

  return result;
}

export function isCloudflareModel(model: string): boolean {
  return model.startsWith('@cf/');
}
