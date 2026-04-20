import type { Db } from "../db";
import { normalizeRoomId } from "../db";
import type { StoredHookEvent } from "../types";
import { embedText } from "./embeddings";
import { buildHookEventSearchText } from "./text";

const INDEX_BATCH_SIZE = 100;

interface EventIndexRow {
  event_id: unknown;
  employee_name: unknown;
  client: unknown;
  repo_root: unknown;
  branch: unknown;
  payload_json: unknown;
}

interface EventResultRow {
  seq: unknown;
  event_id: unknown;
  received_at: unknown;
  employee_user_id: unknown;
  employee_name: unknown;
  client: unknown;
  repo_root: unknown;
  branch: unknown;
  payload_json: unknown;
}

interface SearchResultRow extends EventResultRow {
  score: unknown;
  search_text: unknown;
}

export interface EventQueryFilters {
  organizationId: string;
  roomId?: string;
  employeeName?: string;
  repoRoot?: string;
  branch?: string;
  client?: string;
  since?: string;
  until?: string;
  limit: number;
}

export interface SearchEventResult {
  event: StoredHookEvent;
  score: number;
  search_text: string;
}

export async function indexUnembeddedEvents(db: Db): Promise<number> {
  let indexedCount = 0;

  while (true) {
    const rows = await db.client<EventIndexRow[]>`
      SELECT
        event_id,
        employee_name,
        client,
        repo_root,
        branch,
        payload_json
      FROM hook_events
      WHERE embedding IS NULL
      ORDER BY received_at ASC, seq ASC
      LIMIT ${INDEX_BATCH_SIZE}
    `;

    if (rows.length === 0) {
      return indexedCount;
    }

    for (const row of rows) {
      await indexRow(db, row);
      indexedCount += 1;
    }
  }
}

export async function indexEventById(db: Db, eventId: string): Promise<boolean> {
  const [row] = await db.client<EventIndexRow[]>`
    SELECT
      event_id,
      employee_name,
      client,
      repo_root,
      branch,
      payload_json
    FROM hook_events
    WHERE event_id = ${eventId}
      AND embedding IS NULL
  `;

  if (!row) {
    return false;
  }

  await indexRow(db, row);
  return true;
}

export async function queryOrganizationEvents(
  db: Db,
  filters: EventQueryFilters,
): Promise<StoredHookEvent[]> {
  const rows = await db.client<EventResultRow[]>`
    SELECT
      h.seq,
      h.event_id,
      h.received_at,
      h.employee_user_id,
      COALESCE(NULLIF(BTRIM(u.name), ''), u.email, h.employee_name) AS employee_name,
      h.client,
      h.repo_root,
      h.branch,
      h.payload_json
    FROM hook_events AS h
    INNER JOIN rooms AS r ON r.room_id = h.room_id
    LEFT JOIN "user" AS u ON u.id = h.employee_user_id
    WHERE r.organization_id = ${filters.organizationId}
      AND (${normalizeOptionalRoomId(filters.roomId)}::text IS NULL OR h.room_id = ${normalizeOptionalRoomId(filters.roomId)}::text)
      AND (${filters.employeeName ?? null}::text IS NULL OR COALESCE(NULLIF(BTRIM(u.name), ''), u.email, h.employee_name) = ${filters.employeeName ?? null}::text)
      AND (${filters.repoRoot ?? null}::text IS NULL OR h.repo_root = ${filters.repoRoot ?? null}::text)
      AND (${filters.branch ?? null}::text IS NULL OR h.branch = ${filters.branch ?? null}::text)
      AND (${filters.client ?? null}::text IS NULL OR h.client = ${filters.client ?? null}::text)
      AND (${filters.since ?? null}::timestamptz IS NULL OR h.received_at >= ${filters.since ?? null}::timestamptz)
      AND (${filters.until ?? null}::timestamptz IS NULL OR h.received_at <= ${filters.until ?? null}::timestamptz)
    ORDER BY h.received_at DESC, h.seq DESC
    LIMIT ${filters.limit}
  `;

  return rows.map(mapStoredHookEvent);
}

export async function searchEvents(
  db: Db,
  filters: EventQueryFilters & { query: string },
): Promise<SearchEventResult[]> {
  const embedding = await embedText(filters.query);
  const vector = toVectorLiteral(embedding);

  const rows = await db.client<SearchResultRow[]>`
    SELECT
      h.seq,
      h.event_id,
      h.received_at,
      h.employee_user_id,
      COALESCE(NULLIF(BTRIM(u.name), ''), u.email, h.employee_name) AS employee_name,
      h.client,
      h.repo_root,
      h.branch,
      h.payload_json,
      h.search_text,
      (1 - (h.embedding <=> ${vector}::vector)) AS score
    FROM hook_events AS h
    INNER JOIN rooms AS r ON r.room_id = h.room_id
    LEFT JOIN "user" AS u ON u.id = h.employee_user_id
    WHERE r.organization_id = ${filters.organizationId}
      AND h.embedding IS NOT NULL
      AND (${normalizeOptionalRoomId(filters.roomId)}::text IS NULL OR h.room_id = ${normalizeOptionalRoomId(filters.roomId)}::text)
      AND (${filters.employeeName ?? null}::text IS NULL OR COALESCE(NULLIF(BTRIM(u.name), ''), u.email, h.employee_name) = ${filters.employeeName ?? null}::text)
      AND (${filters.repoRoot ?? null}::text IS NULL OR h.repo_root = ${filters.repoRoot ?? null}::text)
      AND (${filters.branch ?? null}::text IS NULL OR h.branch = ${filters.branch ?? null}::text)
      AND (${filters.client ?? null}::text IS NULL OR h.client = ${filters.client ?? null}::text)
      AND (${filters.since ?? null}::timestamptz IS NULL OR h.received_at >= ${filters.since ?? null}::timestamptz)
      AND (${filters.until ?? null}::timestamptz IS NULL OR h.received_at <= ${filters.until ?? null}::timestamptz)
    ORDER BY h.embedding <=> ${vector}::vector ASC, h.received_at DESC, h.seq DESC
    LIMIT ${filters.limit}
  `;

  return rows.map((row) => ({
    event: mapStoredHookEvent(row),
    score: typeof row.score === "number" ? row.score : Number(row.score),
    search_text: readString(row.search_text, "search_text"),
  }));
}

async function indexRow(db: Db, row: EventIndexRow): Promise<void> {
  const searchText = buildHookEventSearchText({
    employee_name: readString(row.employee_name, "employee_name"),
    client: readString(row.client, "client"),
    repo_root: readString(row.repo_root, "repo_root"),
    branch: row.branch == null ? null : String(row.branch),
    payload: row.payload_json,
  });

  const embedding = await embedText(searchText);
  const vector = toVectorLiteral(embedding);

  await db.client`
    UPDATE hook_events
    SET
      search_text = ${searchText},
      embedding = ${vector}::vector,
      indexed_at = NOW()
    WHERE event_id = ${readString(row.event_id, "event_id")}
  `;
}

function normalizeOptionalRoomId(roomId: string | undefined): string | null {
  if (!roomId) {
    return null;
  }
  return normalizeRoomId(roomId);
}

function mapStoredHookEvent(row: EventResultRow): StoredHookEvent {
  return {
    seq: toNumber(row.seq),
    event_id: readString(row.event_id, "event_id"),
    received_at: toRfc3339(row.received_at),
    employee_user_id: readString(row.employee_user_id, "employee_user_id"),
    employee_name: readString(row.employee_name, "employee_name"),
    client: readString(row.client, "client"),
    repo_root: readString(row.repo_root, "repo_root"),
    branch: row.branch == null ? null : String(row.branch),
    payload: row.payload_json,
  };
}

function toVectorLiteral(values: number[]): string {
  return `[${values.join(",")}]`;
}

function readString(value: unknown, key: string): string {
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
  if (typeof value === "bigint") {
    return Number(value);
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
