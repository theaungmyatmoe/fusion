import { createLLMClient, type ChatMessage } from '../llm/client.js';
import type { Config } from '../config.js';
import { applySearchReplace } from '../util/searchReplace.js';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import chalk from 'chalk';

export interface ToolCall {
  id: string;
  name: string;
  arguments: any;
}

export interface ToolResult {
  tool_call_id: string;
  content: string;
}

export class Agent {
  private llm: ReturnType<typeof createLLMClient>;
  private messages: ChatMessage[] = [];
  private todos: Array<{ content: string; status: 'pending' | 'in_progress' | 'done' }> = [];
  private cwd: string;

  constructor(private config: Config, cwd: string = process.cwd()) {
    this.llm = createLLMClient(config);
    this.cwd = cwd;
  }

  async process(userMessage: string): Promise<string> {
    if (this.messages.length === 0) {
      this.messages.push({
        role: 'system',
        content: `You are Zencode, a powerful, autonomous coding agent optimized for Termux and mobile development.
You have access to tools to explore, understand, and modify codebases safely and efficiently.
Always prefer using get_symbols (LSP-like code intelligence) to understand structure before editing.
Use search_replace for ALL edits — it strictly requires old_string to match exactly once for safety.
Be thoughtful: explore with tools, plan changes, make minimal safe edits, verify when possible.
Use todo tools to track your work visibly.
When done, give a clear, concise final summary.`
      });
    }

    this.messages.push({ role: 'user', content: userMessage });

    const tools = this.getTools();

    const maxRounds = 10;
    for (let round = 0; round < maxRounds; round++) {
      const result = await this.llm.chat({
        messages: this.messages,
        tools,
        temperature: 0.4,
      });

      if (result.reasoning_content) {
        console.log(chalk.hex("#8b5cf6")("🤔 Thinking: ") + chalk.gray(result.reasoning_content));
      }

      if (result.tool_calls && result.tool_calls.length > 0) {
        this.messages.push({ role: 'assistant', content: result.content || '' } as any);

        for (const tc of result.tool_calls) {
          const name = tc.function?.name || (tc as any).name;
          let args: any = {};
          try {
            args = JSON.parse(tc.function?.arguments || (tc as any).arguments || '{}');
          } catch (e) {}

          console.log(chalk.cyan(`[tool] ${name}`), JSON.stringify(args).slice(0, 200));

          let output = '';
          try {
            output = await this.executeTool(name, args);
          } catch (e: any) {
            output = `Tool execution error: ${e.message}`;
          }

          this.messages.push({
            role: 'tool',
            content: `Tool ${name} result:\n${output}`,
          } as any);
        }
        continue;
      }

      // Final response
      const final = result.content || '(no response)';
      this.messages.push({ role: 'assistant', content: final });
      return final;
    }

    return 'Agent reached max rounds without final answer.';
  }

  private getTools() {
    return [
      {
        type: 'function' as const,
        function: {
          name: 'read_file',
          description: 'Read the contents of a file. Use this to understand code before editing.',
          parameters: {
            type: 'object',
            properties: {
              path: { type: 'string', description: 'Path relative to project root or absolute' },
            },
            required: ['path'],
          },
        },
      },
      {
        type: 'function' as const,
        function: {
          name: 'write_file',
          description: 'Create a new file or completely overwrite an existing file with new content.',
          parameters: {
            type: 'object',
            properties: {
              path: { type: 'string', description: 'Path relative to project root' },
              content: { type: 'string', description: 'Complete content to write to the file' },
            },
            required: ['path', 'content'],
          },
        },
      },
      {
        type: 'function' as const,
        function: {
          name: 'search_replace',
          description: 'Precise, safe edit. old_string MUST appear EXACTLY ONCE. This is the only way to edit files.',
          parameters: {
            type: 'object',
            properties: {
              path: { type: 'string' },
              old_string: { type: 'string' },
              new_string: { type: 'string' },
            },
            required: ['path', 'old_string', 'new_string'],
          },
        },
      },
      {
        type: 'function' as const,
        function: {
          name: 'grep',
          description: 'Fast search across the project using ripgrep.',
          parameters: {
            type: 'object',
            properties: {
              pattern: { type: 'string' },
              glob: { type: 'string' },
            },
            required: ['pattern'],
          },
        },
      },
      {
        type: 'function' as const,
        function: {
          name: 'get_symbols',
          description: 'LSP-like code intelligence. Finds functions, classes, interfaces, exports etc. using ripgrep. Call this first to understand the codebase structure like Zed does with LSP.',
          parameters: {
            type: 'object',
            properties: {
              query: { type: 'string' },
              kind: { type: 'string', description: 'function|class|interface|type|const|export' },
            },
            required: ['query'],
          },
        },
      },
      {
        type: 'function' as const,
        function: {
          name: 'todo_write',
          description: 'Create or update the visible todo list to track your work. The user sees these todos.',
          parameters: {
            type: 'object',
            properties: {
              todos: {
                type: 'array',
                items: {
                  type: 'object',
                  properties: {
                    content: { type: 'string' },
                    status: { type: 'string', enum: ['pending', 'in_progress', 'done'] },
                  },
                },
              },
            },
            required: ['todos'],
          },
        },
      },
    ];
  }

  private async executeTool(name: string, args: any): Promise<string> {
    if (name === 'read_file') {
      const fullPath = resolve(this.cwd, args.path);
      if (!existsSync(fullPath)) return `File not found: ${args.path}`;
      return readFileSync(fullPath, 'utf8');
    }

    if (name === 'write_file') {
      const fullPath = resolve(this.cwd, args.path);
      const dir = dirname(fullPath);
      const { mkdirSync } = await import('node:fs');
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }
      writeFileSync(fullPath, args.content, 'utf8');
      return `SUCCESS. Wrote file to ${args.path}.`;
    }

    if (name === 'search_replace') {
      const fullPath = resolve(this.cwd, args.path);
      if (!existsSync(fullPath)) return `File not found: ${args.path}`;
      const content = readFileSync(fullPath, 'utf8');
      const { newContent, result: sr, error } = applySearchReplace(content, args.old_string, args.new_string);
      if (error) return `ERROR: ${error}`;
      writeFileSync(fullPath, newContent);
      return `SUCCESS. Applied edit to ${args.path}.\n${sr.diff}`;
    }

    if (name === 'grep') {
      try {
        const cmd = `rg --line-number --no-heading --color=never ${args.glob ? `--glob '${args.glob}' ` : ''}"${args.pattern}" . || true`;
        return spawnSync('rg', ['--line-number', '--no-heading', '--color=never', ...(args.glob ? ['--glob', args.glob] : []), args.pattern, '.'], {
          encoding: 'utf8',
          cwd: this.cwd,
        }).stdout || 'No matches.';
      } catch (e: any) {
        return e.stdout || 'grep failed (is ripgrep installed?)';
      }
    }

    if (name === 'get_symbols') {
      try {
        const kind = args.kind || '(function|class|interface|type|const|let|export)';
        const cmd = `rg --line-number --no-heading --color=never -e "(${kind})\\s+${args.query}" --glob '*.ts' --glob '*.tsx' --glob '*.js' --glob '*.jsx' . || true`;
        return spawnSync('rg', ['--line-number', '--no-heading', '--color=never', '-e', `(${kind})\\s+${args.query}`, '--glob', '*.ts', '--glob', '*.tsx', '--glob', '*.js', '--glob', '*.jsx', '.'], {
          encoding: 'utf8',
          cwd: this.cwd,
        }).stdout || 'No symbols found.';
      } catch {
        return 'Symbol search failed (ripgrep not available?)';
      }
    }

    if (name === 'todo_write') {
      this.todos = args.todos || [];
      return `Todos updated. Current list:\n${this.todos.map((t: any) => `- [${t.status}] ${t.content}`).join('\n')}`;
    }

    return `Unknown tool: ${name}`;
  }

  getTodos() {
    return this.todos;
  }

  getHistory() {
    return this.messages;
  }
}
