import { randomInt, randomUUID } from "node:crypto";

import { Pool, type QueryResultRow } from "pg";

import {
  type EmployeeSnapshot,
  type HookTurnReport,
  type Room,
  type RoomSnapshot,
  type StoredHookEvent,
  type SummaryStatus,
  emptyRoomSnapshot,
} from "./types.js";

const ROOM_CODE_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const ROOM_CODE_LENGTH = 6;

export class Db {
  readonly pool: Pool;

  private constructor(pool: Pool) {
    this.pool = pool;
  }

  static async connect(databaseUrl: string): Promise<Db> {
    const pool = new Pool({
      connectionString: databaseUrl,
      max: 10,
    });

    try {
      await pool.query("SELECT 1");
    } catch (error) {
      await pool.end().catch(() => undefined);
      throw new Error(`failed to connect to PostgreSQL: ${formatError(error)}`);
    }

    return new Db(pool);
  }

  async close(): Promise<void> {
    await this.pool.end();
  }

  async ping(): Promise<void> {
    await this.pool.query("SELECT 1");
  }

  async createRoom(name: string): Promise<Room> {
    for (let attempt = 0; attempt < 10; attempt += 1) {
      const roomId = generateRoomCode();

      try {
        const result = await this.pool.query(
          `
            INSERT INTO rooms (room_id, name)
            VALUES ($1, $2)
            RETURNING created_at
          `,
          [roomId, name],
        );

        return {
          room_id: roomId,
          name,
          created_at: toRfc3339(result.rows[0]?.created_at),
        };
      } catch (error) {
        if (isUniqueViolation(error)) {
          continue;
        }
        throw new Error(`failed to insert room into PostgreSQL: ${formatError(error)}`);
      }
    }

    throw new Error("failed to generate unique room code after 10 attempts");
  }

  async getRoom(roomId: string): Promise<Room | null> {
    const result = await this.pool.query(
      `
        SELECT room_id, name, created_at
        FROM rooms
        WHERE room_id = $1
      `,
      [normalizeRoomId(roomId)],
    );

    if (!result.rowCount) {
      return null;
    }

    return mapRoom(result.rows[0]);
  }

  async insertHookEvent(roomId: string, report: HookTurnReport): Promise<StoredHookEvent> {
    const normalizedRoomId = normalizeRoomId(roomId);
    const eventId = randomUUID();
    const result = await this.pool.query(
      `
        INSERT INTO hook_events (
          event_id,
          room_id,
          employee_name,
          client,
          repo_root,
          branch,
          payload_json
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING seq, received_at
      `,
      [
        eventId,
        normalizedRoomId,
        report.employee_name,
        report.client,
        report.repo_root,
        report.branch,
        report.payload,
      ],
    );

    return {
      seq: toNumber(result.rows[0]?.seq),
      event_id: eventId,
      received_at: toRfc3339(result.rows[0]?.received_at),
      employee_name: report.employee_name,
      client: report.client,
      repo_root: report.repo_root,
      branch: report.branch,
      payload: report.payload,
    };
  }

  async getHookEvents(
    roomId: string,
    before: number | undefined,
    after: number | undefined,
    limit: number | undefined,
  ): Promise<StoredHookEvent[]> {
    const effectiveLimit = limit ?? Number.MAX_SAFE_INTEGER;
    const result = await this.pool.query(
      `
        SELECT
          seq,
          event_id,
          employee_name,
          client,
          repo_root,
          branch,
          payload_json,
          received_at
        FROM hook_events
        WHERE room_id = $1
          AND ($2::bigint IS NULL OR seq < $2)
          AND ($3::bigint IS NULL OR seq > $3)
        ORDER BY seq DESC
        LIMIT $4
      `,
      [normalizeRoomId(roomId), before ?? null, after ?? null, effectiveLimit],
    );

    return result.rows.map(mapStoredHookEvent);
  }

  async getSummary(roomId: string): Promise<RoomSnapshot> {
    const result = await this.pool.query<{ content_json: RoomSnapshot }>(
      `
        SELECT content_json
        FROM summaries
        WHERE room_id = $1
      `,
      [normalizeRoomId(roomId)],
    );

    return result.rowCount ? normalizeSnapshot(result.rows[0]?.content_json) : emptyRoomSnapshot();
  }

  async getSummaryStatus(roomId: string): Promise<SummaryStatus> {
    const result = await this.pool.query<{ status: SummaryStatus }>(
      `
        SELECT status
        FROM summaries
        WHERE room_id = $1
      `,
      [normalizeRoomId(roomId)],
    );

    return result.rowCount ? parseSummaryStatus(result.rows[0]?.status) : "ready";
  }

  async setSummaryStatus(roomId: string, status: SummaryStatus): Promise<void> {
    await this.pool.query(
      `
        INSERT INTO summaries (room_id, content_json, status, updated_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT(room_id) DO UPDATE SET
          status = EXCLUDED.status,
          updated_at = EXCLUDED.updated_at
      `,
      [normalizeRoomId(roomId), emptyRoomSnapshot(), status],
    );
  }

  async setSummary(roomId: string, content: RoomSnapshot): Promise<void> {
    await this.pool.query(
      `
        INSERT INTO summaries (room_id, content_json, status, updated_at)
        VALUES ($1, $2, 'ready', NOW())
        ON CONFLICT(room_id) DO UPDATE SET
          content_json = EXCLUDED.content_json,
          status = 'ready',
          updated_at = EXCLUDED.updated_at
      `,
      [normalizeRoomId(roomId), normalizeSnapshot(content)],
    );
  }
}

export function normalizeRoomId(roomId: string): string {
  return roomId.trim().toUpperCase();
}

function parseSummaryStatus(value: unknown): SummaryStatus {
  if (value === "generating" || value === "ready" || value === "error") {
    return value;
  }
  throw new Error(`unknown summary status: ${String(value)}`);
}

function mapRoom(row: QueryResultRow): Room {
  return {
    room_id: readString(row, "room_id"),
    name: readString(row, "name"),
    created_at: toRfc3339(row.created_at),
  };
}

function mapStoredHookEvent(row: QueryResultRow): StoredHookEvent {
  return {
    seq: toNumber(row.seq),
    event_id: readString(row, "event_id"),
    received_at: toRfc3339(row.received_at),
    employee_name: readString(row, "employee_name"),
    client: readString(row, "client"),
    repo_root: readString(row, "repo_root"),
    branch: row.branch == null ? null : String(row.branch),
    payload: row.payload_json,
  };
}

function normalizeSnapshot(snapshot: RoomSnapshot | undefined): RoomSnapshot {
  const base = snapshot ?? emptyRoomSnapshot();
  return {
    bluf_markdown: typeof base.bluf_markdown === "string" ? base.bluf_markdown : "",
    overview_markdown: typeof base.overview_markdown === "string" ? base.overview_markdown : "",
    employees: Array.isArray(base.employees)
      ? base.employees.map(normalizeEmployeeSnapshot)
      : [],
  };
}

function normalizeEmployeeSnapshot(snapshot: EmployeeSnapshot): EmployeeSnapshot {
  return {
    employee_name: typeof snapshot.employee_name === "string" ? snapshot.employee_name : "",
    content_markdown:
      typeof snapshot.content_markdown === "string" ? snapshot.content_markdown : "",
    last_update_at: typeof snapshot.last_update_at === "string" ? snapshot.last_update_at : "",
  };
}

function generateRoomCode(): string {
  let roomCode = "";
  for (let index = 0; index < ROOM_CODE_LENGTH; index += 1) {
    roomCode += ROOM_CODE_ALPHABET[randomInt(ROOM_CODE_ALPHABET.length)];
  }
  return roomCode;
}

function readString(row: QueryResultRow, key: string): string {
  const value = row[key];
  if (typeof value !== "string") {
    throw new Error(`failed to decode ${key}`);
  }
  return value;
}

function toNumber(value: unknown): number {
  if (typeof value === "number") {
    return value;
  }
  if (typeof value === "string") {
    return Number.parseInt(value, 10);
  }
  throw new Error(`failed to decode numeric value: ${String(value)}`);
}

function toRfc3339(value: unknown): string {
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (typeof value === "string") {
    return new Date(value).toISOString();
  }
  throw new Error(`failed to decode timestamp: ${String(value)}`);
}

function isUniqueViolation(error: unknown): boolean {
  return Boolean(
    error &&
      typeof error === "object" &&
      "code" in error &&
      (error as { code?: string }).code === "23505",
  );
}

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
