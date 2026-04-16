import type { Db, RoomRecord } from "../db";
import type { FeedStreamHub } from "../sse";
import type { StoragePaths } from "../storage";
import type { ServerConfig } from "../config";
import type { StoredHookEvent } from "../types";
import type { AgentMessage, HostMessage } from "./protocol";
import { applySummaryToolCall } from "./tool-executor";

interface SummaryAgentHostOptions {
  config: ServerConfig;
  db: Db;
  feedHub: FeedStreamHub;
  storage: StoragePaths;
}

type SummaryAgentProcess = Bun.Subprocess<"pipe", "pipe", "pipe">;
const ORGANIZATION_HEARTBEAT_EVENT_LIMIT = 500;

export class SummaryAgentHost {
  private child: SummaryAgentProcess | null = null;
  private heartbeatTimer: Timer | null = null;
  private heartbeatSweepInFlight = false;
  private pendingOrganizationHeartbeatCutoff = new Map<string, string>();
  private starting: Promise<void> | null = null;

  constructor(private readonly options: SummaryAgentHostOptions) {}

  async start(): Promise<void> {
    await this.options.db.resetGeneratingOrganizationSummaries("error");
    await this.ensureRunning();
    this.startHeartbeatTimer();
  }

  async stop(): Promise<void> {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }

    const child = this.child;
    if (!child) {
      return;
    }

    this.detachChild(child);
    child.kill("SIGTERM");
    await child.exited.catch(() => undefined);
  }

  async checkReady(): Promise<void> {
    await this.ensureRunning();
  }

  async enqueue(
    room: Pick<RoomRecord, "name" | "room_id">,
    event: StoredHookEvent,
  ): Promise<void> {
    await this.ensureRunning();
    this.send({
      type: "enqueue_room_event",
      room_id: room.room_id,
      room_name: room.name,
      event,
    });
  }

  private async enqueueOrganizationHeartbeat(
    organizationId: string,
  ): Promise<void> {
    await this.ensureRunning();
    if (this.pendingOrganizationHeartbeatCutoff.has(organizationId)) {
      return;
    }

    const previousSummaryUpdatedAt =
      await this.options.db.getOrganizationSummaryUpdatedAt(organizationId);
    const heartbeatCutoff = new Date().toISOString();
    const [events, rooms] = await Promise.all([
      this.options.db.queryOrganizationEventsForSummary(organizationId, {
        afterReceivedAt: previousSummaryUpdatedAt,
        beforeReceivedAt: heartbeatCutoff,
        limit: ORGANIZATION_HEARTBEAT_EVENT_LIMIT,
      }),
      this.options.db.listRoomsForSummary(organizationId),
    ]);
    const summaryUpdatedAt =
      events.length === ORGANIZATION_HEARTBEAT_EVENT_LIMIT
        ? events.at(-1)?.received_at ?? heartbeatCutoff
        : heartbeatCutoff;
    this.pendingOrganizationHeartbeatCutoff.set(
      organizationId,
      summaryUpdatedAt,
    );
    this.send({
      type: "organization_heartbeat",
      events,
      organization_id: organizationId,
      rooms,
    });
  }

  private async ensureRunning(): Promise<void> {
    if (this.child && !this.child.killed && this.child.exitCode == null) {
      return;
    }
    if (this.starting) {
      return this.starting;
    }

    this.starting = (async () => {
      const { summaryAgent } = this.options.config;
      const child = Bun.spawn({
        cmd: [
          summaryAgent.command,
          ...summaryAgent.args,
          "--codex-home",
          this.options.storage.codexHome,
          "--summary-threads-dir",
          this.options.storage.summaryThreadsDir,
        ],
        cwd: summaryAgent.cwd,
        env: Bun.env,
        stdin: "pipe",
        stdout: "pipe",
        stderr: "pipe",
        onExit: (_subprocess, code, signal, error) => {
          if (error) {
            console.error(`[summary-agent] exit error: ${error.message}`);
          }
          console.error(
            `[summary-agent] exited code=${code ?? "null"} signal=${signal ?? "null"}`,
          );
          this.detachChild(child);
        },
      });

      this.child = child;
      this.startLinePump(child.stdout, (line) => this.handleStdoutLine(line));
      this.startLinePump(child.stderr, (line) => {
        const message = line.trim();
        if (message) {
          console.error(`[summary-agent] ${message}`);
        }
      });
    })();

    try {
      await this.starting;
    } finally {
      this.starting = null;
    }
  }

  private detachChild(child: SummaryAgentProcess): void {
    if (this.child === child) {
      this.child = null;
    }
  }

  private send(message: HostMessage): void {
    if (!this.child?.stdin) {
      throw new Error("summary agent is not running");
    }
    void Promise.resolve(
      this.child.stdin.write(`${JSON.stringify(message)}\n`),
    ).catch((error) => {
      console.error(
        `[summary-agent] failed to write to stdin: ${formatError(error)}`,
      );
    });
  }

  private async handleStdoutLine(line: string): Promise<void> {
    if (!line.trim()) {
      return;
    }

    let message: AgentMessage;
    try {
      message = JSON.parse(line) as AgentMessage;
    } catch (error) {
      console.error(
        `[summary-agent] invalid JSON from child: ${String(error)}`,
      );
      return;
    }

    switch (message.type) {
      case "summary_status": {
        if (message.scope === "organization") {
          await this.options.db.setOrganizationSummaryStatus(
            message.target_id,
            message.status,
          );
          if (message.status === "ready") {
            const pendingCutoff = this.pendingOrganizationHeartbeatCutoff.get(
              message.target_id,
            );
            if (pendingCutoff != null) {
              await this.options.db.setOrganizationSummaryUpdatedAt(
                message.target_id,
                pendingCutoff,
              );
            }
            this.pendingOrganizationHeartbeatCutoff.delete(message.target_id);
          } else if (message.status === "error") {
            this.pendingOrganizationHeartbeatCutoff.delete(message.target_id);
          }
        } else {
          await this.options.db.setRoomSummaryStatus(
            message.target_id,
            message.status,
          );
          this.options.feedHub.publishSummaryStatus(
            message.target_id,
            message.status,
          );
        }
        return;
      }
      case "tool_call": {
        let result;
        try {
          result = await applySummaryToolCall(
            this.options.db,
            message.scope === "organization"
              ? { kind: "organization", organizationId: message.target_id }
              : { kind: "room", roomId: message.target_id },
            message.tool,
            message.arguments,
          );
        } catch (error) {
          result = {
            success: false,
            message: formatError(error),
          };
        }
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
        console.error(
          `[summary-agent] unhandled child message: ${JSON.stringify(neverMessage)}`,
        );
      }
    }
  }

  private startLinePump(
    stream: ReadableStream<Uint8Array>,
    onLine: (line: string) => Promise<void> | void,
  ): void {
    void consumeLines(stream, async (line) => {
      try {
        await onLine(line);
      } catch (error) {
        console.error(
          `[summary-agent] line handler failed: ${formatError(error)}`,
        );
      }
    });
  }

  private startHeartbeatTimer(): void {
    const intervalMs = this.options.config.summaryRefreshIntervalMs;
    if (intervalMs <= 0 || this.heartbeatTimer) {
      return;
    }

    this.heartbeatTimer = setInterval(() => {
      void this.runHeartbeatSweep().catch((error) => {
        console.error(
          `[summary-agent] heartbeat sweep failed: ${formatError(error)}`,
        );
      });
    }, intervalMs);
  }

  private async runHeartbeatSweep(): Promise<void> {
    if (this.heartbeatSweepInFlight) {
      return;
    }

    this.heartbeatSweepInFlight = true;
    try {
      const organizationIds =
        await this.options.db.listOrganizationsWithRooms();
      await Promise.all(
        organizationIds.map(async (organizationId) => {
          const status =
            await this.options.db.getOrganizationSummaryStatus(organizationId);
          if (status === "generating") {
            return;
          }

          await this.enqueueOrganizationHeartbeat(organizationId);
        }),
      );
    } finally {
      this.heartbeatSweepInFlight = false;
    }
  }
}

async function consumeLines(
  stream: ReadableStream<Uint8Array>,
  onLine: (line: string) => Promise<void> | void,
): Promise<void> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let pending = "";

  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) {
        break;
      }

      pending += decoder.decode(value, { stream: true });
      while (true) {
        const newlineIndex = pending.indexOf("\n");
        if (newlineIndex === -1) {
          break;
        }

        const line = pending.slice(0, newlineIndex).replace(/\r$/, "");
        pending = pending.slice(newlineIndex + 1);
        await onLine(line);
      }
    }

    pending += decoder.decode();
    const trailingLine = pending.replace(/\r$/, "");
    if (trailingLine) {
      await onLine(trailingLine);
    }
  } finally {
    reader.releaseLock();
  }
}

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
