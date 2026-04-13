import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import readline from "node:readline";

import type { Db } from "../db.js";
import type { FeedStreamHub } from "../sse.js";
import type { StoragePaths } from "../storage.js";
import type { StoredHookEvent } from "../types.js";
import type { ServerConfig } from "../config.js";
import type { AgentMessage, HostMessage } from "./protocol.js";
import { applySummaryToolCall } from "./tool-executor.js";

interface SummaryAgentHostOptions {
  config: ServerConfig;
  db: Db;
  feedHub: FeedStreamHub;
  storage: StoragePaths;
}

export class SummaryAgentHost {
  private child: ChildProcessWithoutNullStreams | null = null;
  private stdoutReader: readline.Interface | null = null;
  private starting: Promise<void> | null = null;

  constructor(private readonly options: SummaryAgentHostOptions) {}

  async start(): Promise<void> {
    await this.ensureRunning();
  }

  async stop(): Promise<void> {
    if (!this.child) {
      return;
    }

    const child = this.child;
    this.detachChild();
    child.kill("SIGTERM");
  }

  async checkReady(): Promise<void> {
    await this.ensureRunning();
  }

  async enqueue(roomId: string, event: StoredHookEvent): Promise<void> {
    await this.ensureRunning();
    this.send({
      type: "enqueue_event",
      room_id: roomId,
      event,
    });
  }

  private async ensureRunning(): Promise<void> {
    if (this.child && !this.child.killed) {
      return;
    }
    if (this.starting) {
      return this.starting;
    }

    this.starting = (async () => {
      const { summaryAgent } = this.options.config;
      const child = spawn(summaryAgent.command, [...summaryAgent.args, "--codex-home", this.options.storage.codexHome, "--rooms-dir", this.options.storage.roomsDir], {
        cwd: summaryAgent.cwd,
        env: process.env,
        stdio: ["pipe", "pipe", "pipe"],
      });

      child.once("error", (error) => {
        console.error(`[summary-agent] spawn failed: ${error.message}`);
      });
      child.once("exit", (code, signal) => {
        console.error(`[summary-agent] exited code=${code ?? "null"} signal=${signal ?? "null"}`);
        this.detachChild();
      });
      child.stderr.setEncoding("utf8");
      child.stderr.on("data", (chunk: string) => {
        const message = chunk.trim();
        if (message) {
          console.error(`[summary-agent] ${message}`);
        }
      });

      const stdoutReader = readline.createInterface({
        input: child.stdout,
        crlfDelay: Infinity,
      });
      stdoutReader.on("line", (line) => {
        void this.handleStdoutLine(line);
      });

      this.child = child;
      this.stdoutReader = stdoutReader;
    })();

    try {
      await this.starting;
    } finally {
      this.starting = null;
    }
  }

  private detachChild(): void {
    this.stdoutReader?.close();
    this.stdoutReader = null;
    this.child = null;
  }

  private send(message: HostMessage): void {
    if (!this.child?.stdin.writable) {
      throw new Error("summary agent is not running");
    }
    this.child.stdin.write(`${JSON.stringify(message)}\n`);
  }

  private async handleStdoutLine(line: string): Promise<void> {
    if (!line.trim()) {
      return;
    }

    let message: AgentMessage;
    try {
      message = JSON.parse(line) as AgentMessage;
    } catch (error) {
      console.error(`[summary-agent] invalid JSON from child: ${String(error)}`);
      return;
    }

    switch (message.type) {
      case "summary_status": {
        await this.options.db.setSummaryStatus(message.room_id, message.status);
        this.options.feedHub.publishSummaryStatus(message.room_id, message.status);
        return;
      }
      case "tool_call": {
        const result = await applySummaryToolCall(
          this.options.db,
          message.room_id,
          message.tool,
          message.arguments,
        );
        this.send({
          type: "tool_result",
          id: message.id,
          success: result.success,
          message: result.message,
        });
        return;
      }
      default: {
        const neverMessage: never = message;
        console.error(`[summary-agent] unhandled child message: ${JSON.stringify(neverMessage)}`);
      }
    }
  }
}
