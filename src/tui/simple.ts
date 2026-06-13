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
    console.log(
      chalk.gray(
        "Real multi-turn coding agent with tools (read/grep/edit + get_symbols for LSP-like intelligence).\n" +
        "Everything stays in normal scrollback. Use /help."
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
        console.error(chalk.red("Agent error:"), err.message || err);
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
