import {
  type EmployeeSnapshot,
  type HookTurnReport,
  type OrganizationMembership,
  type RoomListEntry,
  type RoomSnapshot,
  type StoredHookEvent,
  type SummaryStatus,
  emptyRoomSnapshot,
} from "./types";
import { CLI_DEVICE_CLIENT_ID } from "./auth";

const ROOM_CODE_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const ROOM_CODE_LENGTH = 6;

interface CreateRoomRow {
  created_at: unknown;
}

interface RoomRow {
  room_id: unknown;
  name: unknown;
  created_at: unknown;
  organization_id: unknown;
  organization_slug: unknown;
  created_by_user_id: unknown;
  bluf_markdown?: unknown;
  employee_count?: unknown;
}

interface OrganizationMembershipRow {
  organization_id: unknown;
  organization_name: unknown;
  organization_slug: unknown;
  role: unknown;
}

interface CliAuthRow {
  has_cli_auth: unknown;
}

interface InsertHookEventRow {
  seq: unknown;
  received_at: unknown;
}

interface HookEventRow {
  seq: unknown;
  event_id: unknown;
  employee_name: unknown;
  client: unknown;
  repo_root: unknown;
  branch: unknown;
  payload_json: unknown;
  received_at: unknown;
}

interface SummaryRow {
  content_json?: RoomSnapshot;
}

interface SummaryStatusRow {
  status: unknown;
}

export interface RoomRecord extends RoomListEntry {
  created_by_user_id: string;
  organization_id: string;
}

export class Db {
  readonly client: Bun.SQL;

  private constructor(client: Bun.SQL) {
    this.client = client;
  }

  static async connect(databaseUrl: string): Promise<Db> {
    const client = new Bun.SQL({
      url: databaseUrl,
      max: 10,
    });

    try {
      await client.connect();
      await client`SELECT 1`;
    } catch (error) {
      await client.close({ timeout: 0 }).catch(() => undefined);
      throw new Error(`failed to connect to PostgreSQL: ${formatError(error)}`);
    }

    return new Db(client);
  }

  async close(): Promise<void> {
    await this.client.close();
  }

  async ping(): Promise<void> {
    await this.client`SELECT 1`;
  }

  async listOrganizationsForUser(userId: string): Promise<OrganizationMembership[]> {
    const rows = await this.client<OrganizationMembershipRow[]>`
      SELECT
        organization.id AS organization_id,
        organization.name AS organization_name,
        organization.slug AS organization_slug,
        member.role AS role
      FROM member
      INNER JOIN organization ON organization.id = member."organizationId"
      WHERE member."userId" = ${userId}
      ORDER BY organization.name ASC, organization."createdAt" ASC
    `;

    return rows.map(mapOrganizationMembership);
  }

  async hasCliAuth(userId: string): Promise<boolean> {
    // Better Auth removes approved device codes after the CLI exchanges them,
    // so we treat either an approved device code or the resulting headerless
    // session row as evidence that the CLI has been configured recently.
    const [row] = await this.client<CliAuthRow[]>`
      SELECT (
        EXISTS(
          SELECT 1
          FROM "deviceCode"
          WHERE "userId" = ${userId}
            AND "clientId" = ${CLI_DEVICE_CLIENT_ID}
            AND status = 'approved'
            AND "expiresAt" > NOW()
        )
        OR EXISTS(
          SELECT 1
          FROM "session"
          WHERE "userId" = ${userId}
            AND "expiresAt" > NOW()
            AND COALESCE("ipAddress", '') = ''
            AND COALESCE("userAgent", '') = ''
        )
      ) AS has_cli_auth
    `;

    return readBoolean(row?.has_cli_auth, "has_cli_auth");
  }

  async getOrganizationMembershipById(
    userId: string,
    organizationId: string,
  ): Promise<OrganizationMembership | null> {
    return this.findOrganizationMembership(userId, "organization_id", organizationId);
  }

  async getOrganizationMembershipBySlug(
    userId: string,
    organizationSlug: string,
  ): Promise<OrganizationMembership | null> {
    return this.findOrganizationMembership(userId, "slug", organizationSlug);
  }

  private async findOrganizationMembership(
    userId: string,
    filterColumn: "organization_id" | "slug",
    filterValue: string,
  ): Promise<OrganizationMembership | null> {
    const [row] = await this.client<OrganizationMembershipRow[]>`
      SELECT
        organization.id AS organization_id,
        organization.name AS organization_name,
        organization.slug AS organization_slug,
        member.role AS role
      FROM member
      INNER JOIN organization ON organization.id = member."organizationId"
      WHERE member."userId" = ${userId}
        AND ${filterColumn === "slug" ? this.client`organization.slug = ${filterValue}` : this.client`member."organizationId" = ${filterValue}`}
    `;

    return row ? mapOrganizationMembership(row) : null;
  }

  async createRoom(
    organizationId: string,
    createdByUserId: string,
    name: string,
  ): Promise<{ room_id: string; name: string; created_at: string }> {
    for (let attempt = 0; attempt < 10; attempt += 1) {
      const roomId = generateRoomCode();

      try {
        const [row] = await this.client<CreateRoomRow[]>`
          INSERT INTO rooms (room_id, organization_id, created_by_user_id, name)
          VALUES (${roomId}, ${organizationId}, ${createdByUserId}, ${name})
          RETURNING created_at
        `;

        return {
          room_id: roomId,
          name,
          created_at: toRfc3339(row?.created_at),
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

  async listRoomsForOrganization(organizationId: string): Promise<RoomListEntry[]> {
    const rows = await this.client<RoomRow[]>`
      SELECT
        rooms.room_id,
        rooms.name,
        rooms.created_at,
        rooms.organization_id,
        organization.slug AS organization_slug,
        rooms.created_by_user_id,
        summaries.content_json->>'bluf_markdown' AS bluf_markdown,
        COALESCE(jsonb_array_length(summaries.content_json->'employees'), 0) AS employee_count
      FROM rooms
      INNER JOIN organization ON organization.id = rooms.organization_id
      LEFT JOIN summaries ON summaries.room_id = rooms.room_id
      WHERE rooms.organization_id = ${organizationId}
      ORDER BY rooms.created_at DESC, rooms.room_id DESC
    `;

    return rows.map((row) => mapRoom(row));
  }

  async getRoomWithAccessCheck(roomId: string, userId: string): Promise<RoomRecord | null> {
    const [row] = await this.client<RoomRow[]>`
      SELECT
        rooms.room_id,
        rooms.name,
        rooms.created_at,
        rooms.organization_id,
        organization.slug AS organization_slug,
        rooms.created_by_user_id
      FROM rooms
      INNER JOIN organization ON organization.id = rooms.organization_id
      INNER JOIN member ON member."organizationId" = rooms.organization_id AND member."userId" = ${userId}
      WHERE rooms.room_id = ${normalizeRoomId(roomId)}
    `;

    return row ? mapRoom(row) : null;
  }

  async getRoom(roomId: string): Promise<RoomRecord | null> {
    const [row] = await this.client<RoomRow[]>`
      SELECT
        rooms.room_id,
        rooms.name,
        rooms.created_at,
        rooms.organization_id,
        organization.slug AS organization_slug,
        rooms.created_by_user_id
      FROM rooms
      INNER JOIN organization ON organization.id = rooms.organization_id
      WHERE rooms.room_id = ${normalizeRoomId(roomId)}
    `;

    return row ? mapRoom(row) : null;
  }

  async insertHookEvent(roomId: string, report: HookTurnReport): Promise<StoredHookEvent> {
    const normalizedRoomId = normalizeRoomId(roomId);
    const eventId = crypto.randomUUID();
    const [row] = await this.client<InsertHookEventRow[]>`
      INSERT INTO hook_events (
        event_id,
        room_id,
        employee_name,
        client,
        repo_root,
        branch,
        payload_json
      )
      VALUES (
        ${eventId},
        ${normalizedRoomId},
        ${report.employee_name},
        ${report.client},
        ${report.repo_root},
        ${report.branch},
        ${report.payload}
      )
      RETURNING seq, received_at
    `;

    return {
      seq: toNumber(row?.seq),
      event_id: eventId,
      received_at: toRfc3339(row?.received_at),
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
    const rows = await this.client<HookEventRow[]>`
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
      WHERE room_id = ${normalizeRoomId(roomId)}
        AND (${before ?? null}::bigint IS NULL OR seq < ${before ?? null})
        AND (${after ?? null}::bigint IS NULL OR seq > ${after ?? null})
      ORDER BY seq DESC
      LIMIT ${effectiveLimit}
    `;

    return rows.map(mapStoredHookEvent);
  }

  async getSummary(roomId: string): Promise<RoomSnapshot> {
    const [row] = await this.client<SummaryRow[]>`
      SELECT content_json
      FROM summaries
      WHERE room_id = ${normalizeRoomId(roomId)}
    `;
    return row ? normalizeSnapshot(row.content_json) : emptyRoomSnapshot();
  }

  async getSummaryStatus(roomId: string): Promise<SummaryStatus> {
    const [row] = await this.client<SummaryStatusRow[]>`
      SELECT status
      FROM summaries
      WHERE room_id = ${normalizeRoomId(roomId)}
    `;
    return row ? parseSummaryStatus(row.status) : "ready";
  }

  async setSummaryStatus(roomId: string, status: SummaryStatus): Promise<void> {
    await this.client`
      INSERT INTO summaries (room_id, content_json, status, updated_at)
      VALUES (${normalizeRoomId(roomId)}, ${emptyRoomSnapshot()}, ${status}, NOW())
      ON CONFLICT(room_id) DO UPDATE SET
        status = EXCLUDED.status,
        updated_at = EXCLUDED.updated_at
    `;
  }

  async setSummary(roomId: string, content: RoomSnapshot): Promise<void> {
    await this.client`
      INSERT INTO summaries (room_id, content_json, status, updated_at)
      VALUES (${normalizeRoomId(roomId)}, ${normalizeSnapshot(content)}, 'ready', NOW())
      ON CONFLICT(room_id) DO UPDATE SET
        content_json = EXCLUDED.content_json,
        status = 'ready',
        updated_at = EXCLUDED.updated_at
    `;
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

function mapOrganizationMembership(row: OrganizationMembershipRow): OrganizationMembership {
  return {
    organization_id: readString(row.organization_id, "organization_id"),
    organization_name: readString(row.organization_name, "organization_name"),
    organization_slug: readString(row.organization_slug, "organization_slug"),
    role: readString(row.role, "role"),
  };
}

function mapRoom(row: RoomRow): RoomRecord {
  return {
    room_id: readString(row.room_id, "room_id"),
    name: readString(row.name, "name"),
    created_at: toRfc3339(row.created_at),
    organization_id: readString(row.organization_id, "organization_id"),
    organization_slug: readString(row.organization_slug, "organization_slug"),
    created_by_user_id: readString(row.created_by_user_id, "created_by_user_id"),
    bluf_markdown: row.bluf_markdown == null ? "" : String(row.bluf_markdown),
    employee_count: row.employee_count == null ? 0 : toNumber(row.employee_count),
  };
}

function mapStoredHookEvent(row: HookEventRow): StoredHookEvent {
  return {
    seq: toNumber(row.seq),
    event_id: readString(row.event_id, "event_id"),
    received_at: toRfc3339(row.received_at),
    employee_name: readString(row.employee_name, "employee_name"),
    client: readString(row.client, "client"),
    repo_root: readString(row.repo_root, "repo_root"),
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
  while (roomCode.length < ROOM_CODE_LENGTH) {
    const bytes = crypto.getRandomValues(new Uint8Array(ROOM_CODE_LENGTH - roomCode.length));
    for (const byte of bytes) {
      if (byte >= 252) {
        continue;
      }
      roomCode += ROOM_CODE_ALPHABET[byte % ROOM_CODE_ALPHABET.length];
      if (roomCode.length === ROOM_CODE_LENGTH) {
        break;
      }
    }
  }
  return roomCode;
}

function readString(value: unknown, key: string): string {
  if (typeof value !== "string") {
    throw new Error(`failed to decode ${key}`);
  }
  return value;
}

function readBoolean(value: unknown, key: string): boolean {
  if (typeof value === "boolean") {
    return value;
  }
  if (typeof value === "string") {
    if (value === "true" || value === "t") {
      return true;
    }
    if (value === "false" || value === "f") {
      return false;
    }
  }
  throw new Error(`failed to decode ${key}`);
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

function isUniqueViolation(error: unknown): boolean {
  if (error instanceof Bun.SQL.PostgresError) {
    return error.code === "23505";
  }
  return false;
}

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
