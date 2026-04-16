import {
  type EmployeeSnapshot,
  type HookTurnReport,
  type OrganizationMembership,
  type OrganizationSnapshot,
  type RoomListEntry,
  type RoomBlufSnapshot,
  type RoomSnapshot,
  type StoredHookEvent,
  type SummaryStatus,
  emptyOrganizationSnapshot,
  emptyRoomSnapshot,
} from "./types";
import { CLI_DEVICE_CLIENT_ID, CLI_USER_AGENT_PREFIX } from "./auth";

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
  member_count: unknown;
  role: unknown;
}

interface CliAuthRow {
  has_cli_auth: boolean;
}

interface InsertHookEventRow {
  seq: unknown;
  received_at: unknown;
}

interface HookEventRow {
  seq: unknown;
  event_id: unknown;
  room_id?: unknown;
  room_name?: unknown;
  employee_name: unknown;
  client: unknown;
  repo_root: unknown;
  branch: unknown;
  payload_json: unknown;
  received_at: unknown;
}

interface SummaryRow {
  content_json?: OrganizationSnapshot;
}

interface RoomSummaryRow {
  content_json?: RoomBlufSnapshot;
  status: unknown;
}

interface SummaryStatusRow {
  status: unknown;
}

interface OrganizationIdRow {
  organization_id: unknown;
}

interface SummaryRoomRecord {
  room_id: string;
  name: string;
}

export interface OrganizationSummaryEvent extends StoredHookEvent {
  room_id: string;
  room_name: string;
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
        organization_member_counts.member_count AS member_count,
        member.role AS role
      FROM member
      INNER JOIN organization ON organization.id = member."organizationId"
      INNER JOIN (
        SELECT
          member."organizationId" AS organization_id,
          COUNT(*)::INT AS member_count
        FROM member
        GROUP BY member."organizationId"
      ) AS organization_member_counts ON organization_member_counts.organization_id = organization.id
      WHERE member."userId" = ${userId}
      ORDER BY organization.name ASC, organization."createdAt" ASC
    `;

    return rows.map(mapOrganizationMembership);
  }

  async hasCliAuth(userId: string): Promise<boolean> {
    // Better Auth removes approved device codes after the CLI exchanges them,
    // so we treat either an approved device code or a session row created by
    // the CLI's explicit user-agent as evidence that the CLI has been
    // configured recently.
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
            AND COALESCE("userAgent", '') LIKE ${`${CLI_USER_AGENT_PREFIX}%`}
        )
      ) AS has_cli_auth
    `;

    return row?.has_cli_auth ?? false;
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
        organization_member_counts.member_count AS member_count,
        member.role AS role
      FROM member
      INNER JOIN organization ON organization.id = member."organizationId"
      INNER JOIN (
        SELECT
          member."organizationId" AS organization_id,
          COUNT(*)::INT AS member_count
        FROM member
        GROUP BY member."organizationId"
      ) AS organization_member_counts ON organization_member_counts.organization_id = organization.id
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
        rooms.created_by_user_id
      FROM rooms
      INNER JOIN organization ON organization.id = rooms.organization_id
      WHERE rooms.organization_id = ${organizationId}
      ORDER BY rooms.created_at DESC, rooms.room_id DESC
    `;

    const [roomBlufs, orgSummary] = await Promise.all([
      this.listRoomBlufsForOrganization(organizationId),
      this.getStoredOrganizationSummary(organizationId),
    ]);
    const roomBlufMap = new Map(
      roomBlufs.map((room) => [normalizeRoomId(room.room_id), room.bluf_markdown]),
    );
    const employeeCounts = countEmployeesByRoom(orgSummary.employees);

    return rows.map((row) => {
      const room = mapRoom(row);
      return {
        ...room,
        bluf_markdown: roomBlufMap.get(room.room_id) ?? "",
        employee_count: employeeCounts.get(room.room_id) ?? 0,
      };
    });
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

  async listOrganizationsWithRooms(): Promise<string[]> {
    const rows = await this.client<OrganizationIdRow[]>`
      SELECT DISTINCT organization_id
      FROM rooms
      ORDER BY organization_id ASC
    `;

    return rows.map((row) => readString(row.organization_id, "organization_id"));
  }

  async listRoomIdsForOrganization(organizationId: string): Promise<string[]> {
    const rows = await this.client<Pick<RoomRow, "room_id">[]>`
      SELECT room_id
      FROM rooms
      WHERE organization_id = ${organizationId}
      ORDER BY created_at DESC, room_id DESC
    `;

    return rows.map((row) => readString(row.room_id, "room_id"));
  }

  async listRoomsForSummary(organizationId: string): Promise<SummaryRoomRecord[]> {
    const rows = await this.client<Pick<RoomRow, "room_id" | "name">[]>`
      SELECT room_id, name
      FROM rooms
      WHERE organization_id = ${organizationId}
      ORDER BY created_at DESC, room_id DESC
    `;

    return rows.map((row) => ({
      room_id: readString(row.room_id, "room_id"),
      name: readString(row.name, "name"),
    }));
  }

  async queryOrganizationEventsForSummary(
    organizationId: string,
    options: {
      employeeName?: string;
      limit?: number;
      roomId?: string;
    } = {},
  ): Promise<OrganizationSummaryEvent[]> {
    const limit = options.limit ?? 50;
    const normalizedRoomId = options.roomId ? normalizeRoomId(options.roomId) : null;
    const rows = await this.client<HookEventRow[]>`
      SELECT
        h.seq,
        h.event_id,
        h.room_id,
        r.name AS room_name,
        h.employee_name,
        h.client,
        h.repo_root,
        h.branch,
        h.payload_json,
        h.received_at
      FROM hook_events AS h
      INNER JOIN rooms AS r ON r.room_id = h.room_id
      WHERE r.organization_id = ${organizationId}
        AND (${normalizedRoomId}::text IS NULL OR h.room_id = ${normalizedRoomId}::text)
        AND (${options.employeeName ?? null}::text IS NULL OR h.employee_name = ${options.employeeName ?? null}::text)
      ORDER BY h.received_at DESC, h.seq DESC
      LIMIT ${limit}
    `;

    return rows.map((row) => ({
      ...mapStoredHookEvent(row),
      room_id: readString(row.room_id, "room_id"),
      room_name: readString(row.room_name, "room_name"),
    }));
  }

  async getOrganizationSummary(
    organizationId: string,
  ): Promise<OrganizationSnapshot> {
    const [snapshot, rooms] = await Promise.all([
      this.getStoredOrganizationSummary(organizationId),
      this.listRoomBlufsForOrganization(organizationId),
    ]);

    return {
      ...snapshot,
      rooms,
    };
  }

  async getRoomSummary(roomId: string): Promise<RoomSnapshot> {
    const room = await this.getRoom(roomId);
    if (!room) {
      return emptyRoomSnapshot();
    }

    const [roomBluf, organizationSummary] = await Promise.all([
      this.getRoomBlufSnapshot(room.room_id),
      this.getStoredOrganizationSummary(room.organization_id),
    ]);

    return {
      bluf_markdown: roomBluf.bluf_markdown,
      employees: organizationSummary.employees.filter((employee) =>
        employee.room_ids.some((entry) => normalizeRoomId(entry) === room.room_id),
      ),
    };
  }

  async getOrganizationSummaryStatus(organizationId: string): Promise<SummaryStatus> {
    const [row] = await this.client<SummaryStatusRow[]>`
      SELECT status
      FROM organization_summaries
      WHERE organization_id = ${organizationId}
    `;
    return row ? parseSummaryStatus(row.status) : "ready";
  }

  async setOrganizationSummaryStatus(organizationId: string, status: SummaryStatus): Promise<void> {
    await this.client`
      INSERT INTO organization_summaries (organization_id, content_json, status, updated_at)
      VALUES (${organizationId}, ${normalizeStoredOrganizationSummary()}, ${status}, NOW())
      ON CONFLICT(organization_id) DO UPDATE SET
        status = EXCLUDED.status,
        updated_at = EXCLUDED.updated_at
    `;
  }

  async setOrganizationSummary(
    organizationId: string,
    content: OrganizationSnapshot,
  ): Promise<void> {
    await this.client`
      INSERT INTO organization_summaries (organization_id, content_json, status, updated_at)
      VALUES (${organizationId}, ${normalizeStoredOrganizationSummary(content)}, 'ready', NOW())
      ON CONFLICT(organization_id) DO UPDATE SET
        content_json = EXCLUDED.content_json,
        status = 'ready',
        updated_at = EXCLUDED.updated_at
    `;
  }

  async listRoomBlufsForOrganization(
    organizationId: string,
  ): Promise<RoomBlufSnapshot[]> {
    const rows = await this.client<
      Array<Pick<RoomRow, "room_id"> & { content_json?: RoomBlufSnapshot }>
    >`
      SELECT
        rooms.room_id,
        room_summaries.content_json
      FROM rooms
      LEFT JOIN room_summaries ON room_summaries.room_id = rooms.room_id
      WHERE rooms.organization_id = ${organizationId}
      ORDER BY rooms.created_at DESC, rooms.room_id DESC
    `;

    return rows.map((row) =>
      normalizeRoomBlufSnapshot(row.content_json, readString(row.room_id, "room_id")),
    );
  }

  async getRoomBlufSnapshot(roomId: string): Promise<RoomBlufSnapshot> {
    const normalizedRoomId = normalizeRoomId(roomId);
    const [row] = await this.client<Pick<RoomSummaryRow, "content_json">[]>`
      SELECT content_json
      FROM room_summaries
      WHERE room_id = ${normalizedRoomId}
    `;

    return normalizeRoomBlufSnapshot(row?.content_json, normalizedRoomId);
  }

  async getRoomSummaryStatus(roomId: string): Promise<SummaryStatus> {
    const [row] = await this.client<Pick<RoomSummaryRow, "status">[]>`
      SELECT status
      FROM room_summaries
      WHERE room_id = ${normalizeRoomId(roomId)}
    `;

    return row ? parseSummaryStatus(row.status) : "ready";
  }

  async setRoomSummaryStatus(roomId: string, status: SummaryStatus): Promise<void> {
    const normalizedRoomId = normalizeRoomId(roomId);
    await this.client`
      INSERT INTO room_summaries (room_id, content_json, status, updated_at)
      VALUES (
        ${normalizedRoomId},
        ${normalizeRoomBlufSnapshot(undefined, normalizedRoomId)},
        ${status},
        NOW()
      )
      ON CONFLICT(room_id) DO UPDATE SET
        status = EXCLUDED.status,
        updated_at = EXCLUDED.updated_at
    `;
  }

  async setRoomSummary(roomId: string, content: RoomBlufSnapshot): Promise<void> {
    const normalizedRoom = normalizeRoomBlufSnapshot(content, roomId);
    await this.client`
      INSERT INTO room_summaries (room_id, content_json, status, updated_at)
      VALUES (${normalizedRoom.room_id}, ${normalizedRoom}, 'ready', NOW())
      ON CONFLICT(room_id) DO UPDATE SET
        content_json = EXCLUDED.content_json,
        status = 'ready',
        updated_at = EXCLUDED.updated_at
    `;
  }

  private async getStoredOrganizationSummary(
    organizationId: string,
  ): Promise<OrganizationSnapshot> {
    const [row] = await this.client<SummaryRow[]>`
      SELECT content_json
      FROM organization_summaries
      WHERE organization_id = ${organizationId}
    `;
    return normalizeStoredOrganizationSummary(row?.content_json);
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
    member_count: toNumber(row.member_count),
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

function normalizeOrganizationSnapshot(
  snapshot: OrganizationSnapshot | undefined,
): OrganizationSnapshot {
  const base = snapshot ?? emptyOrganizationSnapshot();
  return {
    bluf_markdown: typeof base.bluf_markdown === "string" ? base.bluf_markdown : "",
    rooms: Array.isArray(base.rooms)
      ? base.rooms.map((room) => normalizeRoomBlufSnapshot(room))
      : [],
    employees: Array.isArray(base.employees)
      ? base.employees.map(normalizeEmployeeSnapshot)
      : [],
  };
}

function normalizeStoredOrganizationSummary(
  snapshot?: OrganizationSnapshot,
): OrganizationSnapshot {
  const normalized = normalizeOrganizationSnapshot(snapshot);
  return {
    ...normalized,
    rooms: [],
  };
}

function normalizeRoomBlufSnapshot(
  snapshot: RoomBlufSnapshot | undefined,
  fallbackRoomId = "",
): RoomBlufSnapshot {
  return {
    room_id: normalizeRoomId(
      typeof snapshot?.room_id === "string" ? snapshot.room_id : fallbackRoomId,
    ),
    bluf_markdown:
      typeof snapshot?.bluf_markdown === "string" ? snapshot.bluf_markdown : "",
    last_update_at:
      typeof snapshot?.last_update_at === "string" ? snapshot.last_update_at : "",
  };
}

function normalizeEmployeeSnapshot(snapshot: EmployeeSnapshot): EmployeeSnapshot {
  return {
    employee_name: typeof snapshot.employee_name === "string" ? snapshot.employee_name : "",
    room_ids: Array.isArray(snapshot.room_ids)
      ? Array.from(
          new Set(
            snapshot.room_ids
              .filter((roomId): roomId is string => typeof roomId === "string")
              .map(normalizeRoomId),
          ),
        )
      : [],
    bluf_markdown: typeof snapshot.bluf_markdown === "string" ? snapshot.bluf_markdown : "",
    last_update_at: typeof snapshot.last_update_at === "string" ? snapshot.last_update_at : "",
  };
}

function countEmployeesByRoom(employees: EmployeeSnapshot[]): Map<string, number> {
  const counts = new Map<string, number>();

  for (const employee of employees) {
    for (const roomId of employee.room_ids) {
      const normalizedRoomId = normalizeRoomId(roomId);
      counts.set(normalizedRoomId, (counts.get(normalizedRoomId) ?? 0) + 1);
    }
  }

  return counts;
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
