import type { Db } from "../db";
import type { SummaryWorkerConfig } from "../config";
import type { StoragePaths } from "../storage";
import { formatError, type SummaryStatus } from "../types";
import type { AgentMessage, HostMessage } from "./protocol";
import { applySummaryToolCall } from "./tool-executor";

interface SummaryAgentHostOptions {
  config: SummaryWorkerConfig;
  db: Db;
  storage: StoragePaths;
}

type SummaryAgentProcess = Bun.Subprocess<"pipe", "pipe", "pipe">;

const ORGANIZATION_HEARTBEAT_EVENT_LIMIT = 500;
const ROOM_SUMMARY_EVENT_LIMIT = 200;
const ROOM_SUMMARY_SWEEP_LIMIT = 50;

export class SummaryAgentHost {
  private child: SummaryAgentProcess | null = null;
  private organizationHeartbeatTimer: Timer | null = null;
  private roomSweepTimer: Timer | null = null;
  private heartbeatSweepInFlight = false;
  private roomSweepInFlight = false;
  private pendingOrganizationHeartbeatCutoff = new Map<string, string>();
  private pendingRoomSummarySeq = new Map<string, number>();
  private starting: Promise<void> | null = null;

  constructor(private readonly options: SummaryAgentHostOptions) {}

  async start(): Promise<void> {
    await Promise.all([
      this.options.db.resetGeneratingOrganizationSummaries("error"),
      this.options.db.resetGeneratingRoomSummaries("error"),
    ]);
    await this.ensureRunning();
    this.startOrganizationHeartbeatTimer();
    this.startRoomSweepTimer();
    void this.runHeartbeatSweep().catch((error) => {
      console.error(
        `[summary-agent] initial organization heartbeat sweep failed: ${formatError(error)}`,
      );
    });
    void this.runRoomSweep().catch((error) => {
      console.error(
        `[summary-agent] initial room summary sweep failed: ${formatError(error)}`,
      );
    });
  }

  async stop(): Promise<void> {
    if (this.organizationHeartbeatTimer) {
      clearInterval(this.organizationHeartbeatTimer);
      this.organizationHeartbeatTimer = null;
    }
    if (this.roomSweepTimer) {
      clearInterval(this.roomSweepTimer);
      this.roomSweepTimer = null;
    }

    const child = this.child;
    if (!child) {
      return;
    }

    this.detachChild(child);
    child.kill("SIGTERM");
    await child.exited.catch(() => undefined);
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
          this.cleanupPendingWork();
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

  private cleanupPendingWork(): void {
    const hadOrganizations = this.pendingOrganizationHeartbeatCutoff.size > 0;
    const hadRooms = this.pendingRoomSummarySeq.size > 0;
    this.pendingOrganizationHeartbeatCutoff.clear();
    this.pendingRoomSummarySeq.clear();
    if (hadOrganizations) {
      void this.options.db
        .resetGeneratingOrganizationSummaries("error")
        .catch((error) => {
          console.error(
            `[summary-agent] failed to reset generating organization summaries: ${formatError(error)}`,
          );
        });
    }
    if (hadRooms) {
      void this.options.db
        .resetGeneratingRoomSummaries("error")
        .catch((error) => {
          console.error(
            `[summary-agent] failed to reset generating room summaries: ${formatError(error)}`,
          );
        });
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
          await this.persistSummaryStatus(
            message.target_id,
            message.status,
            this.pendingOrganizationHeartbeatCutoff,
            (id, cutoff) =>
              this.options.db.setOrganizationSummaryUpdatedAt(id, cutoff),
            (id, status) =>
              this.options.db.setOrganizationSummaryStatus(id, status),
          );
        } else {
          await this.persistSummaryStatus(
            message.target_id,
            message.status,
            this.pendingRoomSummarySeq,
            (id, seq) =>
              this.options.db.setRoomSummaryLastProcessedSeq(id, seq),
            (id, status) =>
              this.options.db.setRoomSummaryStatus(id, status),
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

  private async persistSummaryStatus<T>(
    targetId: string,
    status: SummaryStatus,
    pending: Map<string, T>,
    commitPending: (id: string, value: T) => Promise<void>,
    setStatus: (id: string, status: SummaryStatus) => Promise<void>,
  ): Promise<void> {
    if (status === "ready") {
      const pendingValue = pending.get(targetId);
      if (pendingValue !== undefined) {
        await commitPending(targetId, pendingValue);
      }
    }
    await setStatus(targetId, status);
    if (status === "ready" || status === "error") {
      pending.delete(targetId);
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

  private startOrganizationHeartbeatTimer(): void {
    const intervalMs = this.options.config.organizationSummaryRefreshIntervalMs;
    if (intervalMs <= 0 || this.organizationHeartbeatTimer) {
      return;
    }

    this.organizationHeartbeatTimer = setInterval(() => {
      void this.runHeartbeatSweep().catch((error) => {
        console.error(
          `[summary-agent] organization heartbeat sweep failed: ${formatError(error)}`,
        );
      });
    }, intervalMs);
  }

  private startRoomSweepTimer(): void {
    const intervalMs = this.options.config.roomSummaryPollIntervalMs;
    if (intervalMs <= 0 || this.roomSweepTimer) {
      return;
    }

    this.roomSweepTimer = setInterval(() => {
      void this.runRoomSweep().catch((error) => {
        console.error(
          `[summary-agent] room summary sweep failed: ${formatError(error)}`,
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
          try {
            if (this.pendingOrganizationHeartbeatCutoff.has(organizationId)) {
              return;
            }

            const previousSummaryUpdatedAt =
              await this.options.db.tryStartOrganizationSummary(organizationId);
            if (previousSummaryUpdatedAt == null) {
              return;
            }

            await this.enqueueOrganizationHeartbeat(
              organizationId,
              previousSummaryUpdatedAt,
            );
          } catch (error) {
            console.error(
              `[summary-agent] failed to enqueue organization heartbeat for ${organizationId}: ${formatError(error)}`,
            );
            await this.options.db
              .setOrganizationSummaryStatus(organizationId, "error")
              .catch(() => undefined);
            this.pendingOrganizationHeartbeatCutoff.delete(organizationId);
          }
        }),
      );
    } finally {
      this.heartbeatSweepInFlight = false;
    }
  }

  private async runRoomSweep(): Promise<void> {
    if (this.roomSweepInFlight) {
      return;
    }

    this.roomSweepInFlight = true;
    try {
      const rooms =
        await this.options.db.listRoomsNeedingSummary(ROOM_SUMMARY_SWEEP_LIMIT);
      await Promise.all(
        rooms.map(async (room) => {
          try {
            if (this.pendingRoomSummarySeq.has(room.room_id)) {
              return;
            }

            const claim = await this.options.db.tryStartRoomSummary(
              room.room_id,
            );
            if (!claim) {
              return;
            }

            const events = await this.options.db.queryRoomEventsForSummary(
              room.room_id,
              {
                afterSeq: claim.lastProcessedSeq,
                limit: ROOM_SUMMARY_EVENT_LIMIT,
              },
            );
            if (events.length === 0) {
              await this.options.db.setRoomSummaryStatus(
                room.room_id,
                "ready",
              );
              return;
            }

            this.pendingRoomSummarySeq.set(
              room.room_id,
              events[events.length - 1]!.seq,
            );
            await this.ensureRunning();
            for (const event of events) {
              this.send({
                type: "enqueue_room_event",
                room_id: room.room_id,
                room_name: room.name,
                event,
              });
            }
          } catch (error) {
            console.error(
              `[summary-agent] failed to enqueue room summary for ${room.room_id}: ${formatError(error)}`,
            );
            await this.options.db
              .setRoomSummaryStatus(room.room_id, "error")
              .catch(() => undefined);
            this.pendingRoomSummarySeq.delete(room.room_id);
          }
        }),
      );
    } finally {
      this.roomSweepInFlight = false;
    }
  }

  private async enqueueOrganizationHeartbeat(
    organizationId: string,
    previousSummaryUpdatedAt: string | null,
  ): Promise<void> {
    await this.ensureRunning();

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
