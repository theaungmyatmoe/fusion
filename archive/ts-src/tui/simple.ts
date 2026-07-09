import chalk from "chalk";
import Enquirer from "enquirer";
import type { Config } from "../config.js";
import { Agent } from "../agent/Agent.js";

export class SimpleREPL {
  private agent: Agent;
  private mode: "Normal" | "Plan" | "YOLO";
  private yolo: boolean = false;
  private model: string;

  constructor(private cfg: Config) {
    this.yolo = !!cfg.yolo;
    this.mode = this.yolo ? "YOLO" : "Normal";
    this.model = cfg.model;
    this.agent = new Agent(cfg);
  }

  async run(): Promise<void> {
    const isTerm = isTermux();
    const platform = isTerm ? "Termux (mobile)" : "terminal";

    console.log(
      chalk.hex("#7c3aed").bold("zencode") +
        chalk.gray(`  mobile-first coding agent for ${platform}`)
    );
    console.log(`model: ${this.model}   mode: ${this.mode}`);
    if ((this.cfg as any).configPath) {
      console.log(chalk.gray(`config: ${(this.cfg as any).configPath}`));
    }

    // Pre-flight warning for the common case (global config + Cloudflare Kimi)
    const c = this.cfg as any;
    const needsCf = this.model.startsWith('@cf/') || c.provider === 'cloudflare' || this.model.includes('kimi');
    if (needsCf && !c.cloudflareAccountId) {
      console.log(chalk.yellow('⚠  No Cloudflare credentials found for the default Kimi route.'));
      console.log(chalk.gray('   The reported config file was read but did not supply accountId/apiKey, and no matching env vars were present.'));
      console.log(chalk.gray('   Fix options:'));
      console.log(chalk.gray('     1. Add to your ~/.zshrc (or ~/.zprofile) and restart the terminal:'));
      console.log(chalk.gray('          export CLOUDFLARE_ACCOUNT_ID=6838bf50a0d8548d5945008dc7b6797c'));
      console.log(chalk.gray('          export CLOUDFLARE_API_TOKEN=cfat_...your-full-token'));
      console.log(chalk.gray('     2. Edit the config file above and ensure it contains:'));
      console.log(chalk.gray('          "provider": { "cloudflare": { "options": { "accountId": "...", "apiKey": "..." } } }'));
      console.log(chalk.gray('   Env vars (from .zshrc etc.) always win over the json file.'));
      console.log();
    }

    console.log(
      chalk.gray(
        "Everything stays in normal scrollback (finger scroll + copy work great). Use /help."
      )
    );
    console.log();

    const enquirer = new (Enquirer as any)();

    while (true) {
      let answer: any;
      try {
        answer = await enquirer.prompt({
          type: "input",
          name: "cmd",
          message: chalk.hex("#7c3aed")("z>"),
        });
      } catch {
        console.log(chalk.gray("bye"));
        break;
      }

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

      try {
        const final = await this.agent.process(trimmed);
        console.log(chalk.green("Agent:") + " " + final);

        const todos = this.agent.getTodos ? this.agent.getTodos() : [];
        if (todos.length > 0) {
          console.log(chalk.yellow("Todos:"));
          todos.forEach((t: any) => {
            const icon = t.status === 'done' ? '✓' : t.status === 'in_progress' ? '→' : '○';
            console.log(`  ${icon} ${t.content}`);
          });
        }
      } catch (err: any) {
        const msg = (err?.message || String(err)).trim();
        console.log(chalk.red("Agent error: ") + msg.split('\n')[0]);
        // If the error already contains the Tip from the LLM layer, don't duplicate.
        if (!/Tip:/.test(msg)) {
          console.log(chalk.gray("   Check the config path above, your ~/.zshrc exports, or use /status."));
        }
      }
    }
  }

  private printUser(text: string) {
    console.log(chalk.hex("#3b82f6").bold("You:") + " " + text);
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
      const c: any = this.cfg;
      const hasCf = !!c.cloudflareAccountId;
      const hasKey = !!(c.apiKey && c.apiKey.length > 4);
      console.log(`model=${this.model}  mode=${this.mode}  yolo=${this.yolo}`);
      console.log(`config=${c.configPath || '(none)'}  cfCreds=${hasCf ? 'yes' : 'NO'}  apiKey=${hasKey ? 'set' : 'missing'}`);
      if (!hasCf && (this.model.startsWith('@cf/') || this.model.includes('kimi'))) {
        console.log(chalk.yellow('  → Cloudflare creds missing. See the warning above or put them in the json / env.'));
      }
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
