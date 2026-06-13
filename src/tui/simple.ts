import chalk from "chalk";
import Enquirer from "enquirer";
import { applySearchReplace } from "../util/searchReplace.js";
import type { Config } from "../config.js";
import { createLLMClient, type ChatMessage } from "../llm/client.js";

export class SimpleREPL {
  private yolo: boolean;
  private mode: "Normal" | "Plan" | "YOLO";
  private model: string;
  private llm: ReturnType<typeof createLLMClient>;
  private todos: Array<{ content: string; status: 'pending' | 'in_progress' | 'done' }> = [];

  constructor(private cfg: Config) {
    this.yolo = cfg.yolo ?? false;
    this.mode = this.yolo ? "YOLO" : "Normal";
    this.model = cfg.model;
    this.llm = createLLMClient(cfg); // real LLM client (defaults to Kimi 2.7 on Cloudflare)
  }

  async run(): Promise<void> {
    const isTerm = isTermux();
    const platform = isTerm ? "Termux (mobile)" : "terminal";

    console.log(
      chalk.hex("#7c3aed").bold("zencode") +
        chalk.gray(`  mobile-first coding agent for ${platform}`)
    );
    console.log(`model: ${this.model}   mode: ${this.mode}`);
    if (this.cfg.configPath) {
      console.log(chalk.gray(`config: ${this.cfg.configPath}`));
    }
    console.log(
      chalk.gray(
        "Everything stays in normal scrollback (finger scroll + copy work great). Use /help."
      )
    );
    console.log();

    const enquirer = new (Enquirer as any)();

    // eslint-disable-next-line no-constant-condition
    while (true) {
      try {
        const answer = await enquirer.prompt({
          type: "input",
          name: "cmd",
          message: chalk.hex("#7c3aed")("z>"),
        });
        const trimmed = (answer.cmd || "").trim();
        if (!trimmed) continue;

        if (trimmed === "/exit" || trimmed === "/quit") {
          console.log(chalk.gray("bye"));
          break;
        }

        if (trimmed.startsWith("/")) {
          await this.handleSlash(trimmed);
          continue;
        }

        this.printUser(trimmed);
        await this.simulateAgent(trimmed);
      } catch {
        console.log(chalk.gray("bye"));
        break;
      }
    }
  }

  private printUser(text: string) {
    console.log(chalk.hex("#3b82f6").bold("You:") + " " + text);
  }

  private printTodos() {
    if (this.todos.length === 0) return;
    console.log(chalk.yellow("Todos:"));
    this.todos.forEach((t, i) => {
      const icon = t.status === 'done' ? '✓' : t.status === 'in_progress' ? '→' : '○';
      const color = t.status === 'done' ? chalk.green : t.status === 'in_progress' ? chalk.cyan : chalk.gray;
      console.log(`  ${color(icon)} ${t.content}`);
    });
  }

  private async simulateAgent(userMsg: string) {
    const lower = userMsg.toLowerCase();

    // Fake thinking
    process.stdout.write(chalk.gray("Agent thinking..."));
    await new Promise((r) => setTimeout(r, 90));
    process.stdout.write("\r" + " ".repeat(30) + "\r");

    if (lower.includes("todo") || lower.includes("plan")) {
      console.log(chalk.green("Agent:") + " Tracking todos (visible below).");
      this.todos = [
        { content: "Understand the request", status: 'done' },
        { content: "Show nice mobile-first output", status: 'in_progress' },
        { content: "Wire real search_replace + shell + LLM", status: 'pending' },
      ];
      this.printTodos();
      console.log();
      return;
    }

    if (lower.includes("edit") || lower.includes("fix") || lower.includes("change")) {
      console.log(chalk.green("Agent:") + " Using precise search_replace (old_string must appear exactly once).");

      // Actually run the safety logic with a demo
      const demo = `function greet() {\n  console.log("hello");\n}`;
      const oldS = `  console.log("hello");`;
      const newS = `  console.log("hello from zencode on Termux");\n  // agent made this change`;

      const { result, error } = applySearchReplace(demo, oldS, newS);

      if (error) {
        console.log(chalk.red("  Replace rejected: " + error));
      } else {
        console.log(chalk.hex("#f59e0b")("```diff"));
        console.log(result.diff);
        console.log(chalk.hex("#f59e0b")("```"));
      }
      console.log();

      console.log(chalk.green("  (demo of search_replace safety - in real run it would prompt for approval unless --yolo)"));
      // For full interactive test, run without piping and type a real prompt after setting API keys.
      return;
    }

    // Real call to the configured model (defaults to Kimi 2.7 on Cloudflare Workers AI)
    try {
      const messages: ChatMessage[] = [
        {
          role: "system",
          content:
            "You are Zencode, a helpful coding agent optimized for Termux on mobile. " +
            "Be concise but useful. When suggesting code changes, prefer small precise edits.",
        },
        { role: "user", content: userMsg },
      ];

      const result = await this.llm.chat({ messages, temperature: 0.6 });

      console.log(chalk.green("Agent:") + " " + (result.content || "(no response)"));
    } catch (err: any) {
      console.log(chalk.red("LLM error:"), err?.message || err);
      console.log(chalk.gray("Tip: Make sure you have CLOUDFLARE_ACCOUNT_ID + CLOUDFLARE_API_TOKEN (or ZENCODE_API_KEY) set for the default Kimi route."));
    }
    console.log();
    this.printTodos();
  }

  private async handleSlash(cmd: string): Promise<boolean> {
    const c = cmd.toLowerCase();

    if (c === "/help" || c === "/h" || c === "/?") {
      console.log(chalk.hex("#7c3aed")("Zencode commands (mobile-friendly):"));
      console.log("  /help            show this");
      console.log("  /yolo            toggle auto-approve (use carefully)");
      console.log("  /plan            enter plan mode (think before code)");
      console.log("  /model <name>    switch model (e.g. grok-3)");
      console.log("  /status          current settings");
      console.log("  /exit, /quit     leave");
      console.log();
      console.log(chalk.gray("Just type to chat with the agent."));
      return true;
    }

    if (c === "/yolo") {
      this.yolo = !this.yolo;
      this.mode = this.yolo ? "YOLO" : "Normal";
      console.log(this.yolo ? chalk.yellow("YOLO mode ON") : "YOLO mode OFF");
      return true;
    }

    if (c === "/plan") {
      console.log(chalk.hex("#7c3aed")("Plan mode (stub):") + " agent will only explore. Real version will produce a reviewable plan.");
      return true;
    }

    if (c.startsWith("/model")) {
      const parts = cmd.split(" ");
      if (parts[1]) {
        this.model = parts.slice(1).join(" ");
        console.log(`Model → ${this.model}`);
      } else {
        console.log(`Current model: ${this.model}`);
        console.log("Example: /model grok-3");
      }
      return true;
    }

    if (c === "/status") {
      console.log(`model=${this.model}  mode=${this.mode}  yolo=${this.yolo}`);
      return true;
    }

    if (c === "/exit" || c === "/quit" || c === "/q") {
      console.log("Exiting...");
      process.exit(0);
    }

    console.log(chalk.gray("Unknown command. /help for list."));
    return true;
  }
}

function isTermux(): boolean {
  return !!process.env.TERMUX_VERSION || (process.env.PREFIX || "").includes("com.termux");
}
