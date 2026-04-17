import {
  type EmployeeSnapshot,
  type HookTurnReport,
  type OrganizationMembership,
  type OrganizationSnapshot,
  type RoomListEntry,
  type RoomBlufSnapshot,
  type RoomSummaryResponse,
  type RoomSnapshot,
  type StoredHookEvent,
  type SummaryStatus,
  emptyOrganizationSnapshot,
  emptyRoomSnapshot,
  formatError,
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

interface CountRow {
  count: unknown;
}

interface SummaryRow {
  content_json?: OrganizationSnapshot;
  updated_at?: unknown;
}

interface RoomSummaryRow {
  room_id?: unknown;
  content_json?: RoomSnapshot;
  last_processed_seq?: unknown;
  status: unknown;
  updated_at?: unknown;
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

  async listOrganizationsForUser(
    userId: string,
  ): Promise<OrganizationMembership[]> {
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
    return this.findOrganizationMembership(
      userId,
      "organization_id",
      organizationId,
    );
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
        throw new Error(
          `failed to insert room into PostgreSQL: ${formatError(error)}`,
        );
      }
    }

    throw new Error("failed to generate unique room code after 10 attempts");
  }

  async listRoomsForOrganization(
    organizationId: string,
  ): Promise<RoomListEntry[]> {
    const [rows, roomSummaries] = await Promise.all([
      this.client<RoomRow[]>`
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
      `,
      this.listStoredRoomSummariesForOrganization(organizationId),
    ]);

    const roomBlufMap = new Map(
      roomSummaries.map((room) => [room.room_id, room.snapshot.bluf_markdown]),
    );
    const employeeCounts = new Map(
      roomSummaries.map((room) => [
        room.room_id,
        room.snapshot.employees.length,
      ]),
    );

    return rows.map((row) => {
      const room = mapRoom(row);
      return {
        ...room,
        bluf_markdown: roomBlufMap.get(room.room_id) ?? "",
        employee_count: employeeCounts.get(room.room_id) ?? 0,
      };
    });
  }

  async getRoomWithAccessCheck(
    roomId: string,
    userId: string,
  ): Promise<RoomRecord | null> {
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

  async insertHookEvent(
    roomId: string,
    report: HookTurnReport,
  ): Promise<StoredHookEvent> {
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

  async countHookEvents(roomId: string): Promise<number> {
    const [row] = await this.client<CountRow[]>`
      SELECT COUNT(*)::INT AS count
      FROM hook_events
      WHERE room_id = ${normalizeRoomId(roomId)}
    `;

    return row == null ? 0 : toNumber(row.count);
  }

  async listOrganizationsWithRooms(): Promise<string[]> {
    const rows = await this.client<OrganizationIdRow[]>`
      SELECT DISTINCT organization_id
      FROM rooms
      ORDER BY organization_id ASC
    `;

    return rows.map((row) =>
      readString(row.organization_id, "organization_id"),
    );
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

  async listRoomsForSummary(
    organizationId: string,
  ): Promise<SummaryRoomRecord[]> {
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
      afterReceivedAt?: string | null;
      beforeReceivedAt?: string | null;
      employeeName?: string;
      limit?: number;
      roomId?: string;
    } = {},
  ): Promise<OrganizationSummaryEvent[]> {
    const afterReceivedAt = options.afterReceivedAt ?? null;
    const beforeReceivedAt = options.beforeReceivedAt ?? null;
    const normalizedRoomId = options.roomId
      ? normalizeRoomId(options.roomId)
      : null;
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
        AND (${afterReceivedAt}::timestamptz IS NULL OR h.received_at > ${afterReceivedAt}::timestamptz)
        AND (${beforeReceivedAt}::timestamptz IS NULL OR h.received_at <= ${beforeReceivedAt}::timestamptz)
        AND (${normalizedRoomId}::text IS NULL OR h.room_id = ${normalizedRoomId}::text)
        AND (${options.employeeName ?? null}::text IS NULL OR h.employee_name = ${options.employeeName ?? null}::text)
      ORDER BY h.received_at ASC, h.seq ASC
      ${options.limit == null ? this.client`` : this.client`LIMIT ${options.limit}`}
    `;

    return rows.map((row) => ({
      ...mapStoredHookEvent(row),
      room_id: readString(row.room_id, "room_id"),
      room_name: readString(row.room_name, "room_name"),
    }));
  }

  async queryRoomEventsForSummary(
    roomId: string,
    options: {
      afterSeq?: number;
      limit?: number;
    } = {},
  ): Promise<StoredHookEvent[]> {
    const normalizedRoomId = normalizeRoomId(roomId);
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
      WHERE room_id = ${normalizedRoomId}
        AND (${options.afterSeq ?? null}::bigint IS NULL OR seq > ${options.afterSeq ?? null})
      ORDER BY seq ASC
      ${options.limit == null ? this.client`` : this.client`LIMIT ${options.limit}`}
    `;

    return rows.map(mapStoredHookEvent);
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
    return this.getStoredRoomSummary(normalizeRoomId(roomId));
  }

  async getRoomSummaryResponse(roomId: string): Promise<RoomSummaryResponse> {
    const normalizedRoomId = normalizeRoomId(roomId);
    const [row] = await this.client<RoomSummaryRow[]>`
      SELECT content_json, last_processed_seq, status
      FROM room_summaries
      WHERE room_id = ${normalizedRoomId}
    `;

    return {
      last_processed_seq:
        row?.last_processed_seq == null ? 0 : toNumber(row.last_processed_seq),
      status: row ? parseSummaryStatus(row.status) : "ready",
      summary: normalizeStoredRoomSummary(row?.content_json),
    };
  }

  async getOrganizationSummaryStatus(
    organizationId: string,
  ): Promise<SummaryStatus> {
    const [row] = await this.client<SummaryStatusRow[]>`
      SELECT status
      FROM organization_summaries
      WHERE organization_id = ${organizationId}
    `;
    return row ? parseSummaryStatus(row.status) : "ready";
  }

  async getOrganizationSummaryUpdatedAt(
    organizationId: string,
  ): Promise<string | null> {
    const [row] = await this.client<Pick<SummaryRow, "updated_at">[]>`
      SELECT updated_at
      FROM organization_summaries
      WHERE organization_id = ${organizationId}
    `;

    return row?.updated_at == null ? null : toRfc3339(row.updated_at);
  }

  async setOrganizationSummaryStatus(
    organizationId: string,
    status: SummaryStatus,
  ): Promise<void> {
    await this.client`
      INSERT INTO organization_summaries (organization_id, content_json, status, updated_at)
      VALUES (
        ${organizationId},
        ${normalizeStoredOrganizationSummary()},
        ${status},
        TO_TIMESTAMP(0)
      )
      ON CONFLICT(organization_id) DO UPDATE SET
        status = EXCLUDED.status
    `;
  }

  async resetGeneratingOrganizationSummaries(
    nextStatus: Extract<SummaryStatus, "error" | "ready"> = "error",
  ): Promise<void> {
    await this.client`
      UPDATE organization_summaries
      SET status = ${nextStatus}
      WHERE status = 'generating'
    `;
  }

  async tryStartOrganizationSummary(
    organizationId: string,
  ): Promise<string | null> {
    const [row] = await this.client<Pick<SummaryRow, "updated_at">[]>`
      INSERT INTO organization_summaries (organization_id, content_json, status, updated_at)
      VALUES (
        ${organizationId},
        ${normalizeStoredOrganizationSummary()},
        'generating',
        TO_TIMESTAMP(0)
      )
      ON CONFLICT(organization_id) DO UPDATE SET
        status = 'generating'
      WHERE organization_summaries.status <> 'generating'
      RETURNING updated_at
    `;

    return row?.updated_at == null ? null : toRfc3339(row.updated_at);
  }

  async setOrganizationSummary(
    organizationId: string,
    content: OrganizationSnapshot,
  ): Promise<void> {
    await this.client`
      INSERT INTO organization_summaries (organization_id, content_json, status, updated_at)
      VALUES (
        ${organizationId},
        ${normalizeStoredOrganizationSummary(content)},
        'ready',
        TO_TIMESTAMP(0)
      )
      ON CONFLICT(organization_id) DO UPDATE SET
        content_json = EXCLUDED.content_json
    `;
  }

  async setOrganizationSummaryUpdatedAt(
    organizationId: string,
    updatedAt: string,
  ): Promise<void> {
    await this.client`
      INSERT INTO organization_summaries (
        organization_id,
        content_json,
        updated_at
      )
      VALUES (
        ${organizationId},
        ${normalizeStoredOrganizationSummary()},
        ${updatedAt}::timestamptz
      )
      ON CONFLICT(organization_id) DO UPDATE SET
        updated_at = GREATEST(
          organization_summaries.updated_at,
          EXCLUDED.updated_at
        )
    `;
  }

  async listRoomBlufsForOrganization(
    organizationId: string,
  ): Promise<RoomBlufSnapshot[]> {
    const rows =
      await this.listStoredRoomSummariesForOrganization(organizationId);
    return rows.map((row) =>
      toRoomBlufSnapshot(row.room_id, row.snapshot, row.updated_at),
    );
  }

  async getRoomBlufSnapshot(roomId: string): Promise<RoomBlufSnapshot> {
    const normalizedRoomId = normalizeRoomId(roomId);
    const [row] = await this.client<
      Pick<RoomSummaryRow, "content_json" | "updated_at">[]
    >`
      SELECT content_json, updated_at
      FROM room_summaries
      WHERE room_id = ${normalizedRoomId}
    `;

    return toRoomBlufSnapshot(
      normalizedRoomId,
      row?.content_json,
      row?.updated_at,
    );
  }

  async getRoomSummaryStatus(roomId: string): Promise<SummaryStatus> {
    const [row] = await this.client<Pick<RoomSummaryRow, "status">[]>`
      SELECT status
      FROM room_summaries
      WHERE room_id = ${normalizeRoomId(roomId)}
    `;

    return row ? parseSummaryStatus(row.status) : "ready";
  }

  async setRoomSummaryStatus(
    roomId: string,
    status: SummaryStatus,
  ): Promise<void> {
    const normalizedRoomId = normalizeRoomId(roomId);
    await this.client`
      INSERT INTO room_summaries (
        room_id,
        content_json,
        status,
        updated_at,
        last_processed_seq
      )
      VALUES (
        ${normalizedRoomId},
        ${normalizeStoredRoomSummary()},
        ${status},
        TO_TIMESTAMP(0),
        0
      )
      ON CONFLICT(room_id) DO UPDATE SET
        status = EXCLUDED.status
    `;
  }

  async resetGeneratingRoomSummaries(
    nextStatus: Extract<SummaryStatus, "error" | "ready"> = "error",
  ): Promise<void> {
    await this.client`
      UPDATE room_summaries
      SET status = ${nextStatus}
      WHERE status = 'generating'
    `;
  }

  async setRoomSummary(roomId: string, content: RoomSnapshot): Promise<void> {
    const normalizedRoomId = normalizeRoomId(roomId);
    const normalizedSummary = normalizeStoredRoomSummary(content);
    await this.client`
      INSERT INTO room_summaries (
        room_id,
        content_json,
        status,
        updated_at,
        last_processed_seq
      )
      VALUES (
        ${normalizedRoomId},
        ${normalizedSummary},
        'ready',
        NOW(),
        0
      )
      ON CONFLICT(room_id) DO UPDATE SET
        content_json = EXCLUDED.content_json,
        updated_at = EXCLUDED.updated_at
    `;
  }

  async setRoomSummaryLastProcessedSeq(
    roomId: string,
    lastProcessedSeq: number,
  ): Promise<void> {
    const normalizedRoomId = normalizeRoomId(roomId);
    await this.client`
      INSERT INTO room_summaries (
        room_id,
        content_json,
        status,
        updated_at,
        last_processed_seq
      )
      VALUES (
        ${normalizedRoomId},
        ${normalizeStoredRoomSummary()},
        'ready',
        TO_TIMESTAMP(0),
        ${lastProcessedSeq}
      )
      ON CONFLICT(room_id) DO UPDATE SET
        last_processed_seq = GREATEST(
          room_summaries.last_processed_seq,
          EXCLUDED.last_processed_seq
        )
    `;
  }

  async tryStartRoomSummary(
    roomId: string,
  ): Promise<{ lastProcessedSeq: number } | null> {
    const normalizedRoomId = normalizeRoomId(roomId);
    const [row] = await this.client<Pick<RoomSummaryRow, "last_processed_seq">[]>`
      INSERT INTO room_summaries (
        room_id,
        content_json,
        status,
        updated_at,
        last_processed_seq
      )
      VALUES (
        ${normalizedRoomId},
        ${normalizeStoredRoomSummary()},
        'generating',
        TO_TIMESTAMP(0),
        0
      )
      ON CONFLICT(room_id) DO UPDATE SET
        status = 'generating'
      WHERE room_summaries.status <> 'generating'
      RETURNING last_processed_seq
    `;

    if (!row) {
      return null;
    }

    return {
      lastProcessedSeq:
        row.last_processed_seq == null ? 0 : toNumber(row.last_processed_seq),
    };
  }

  async listRoomsNeedingSummary(limit: number): Promise<SummaryRoomRecord[]> {
    const rows = await this.client<Pick<RoomRow, "room_id" | "name">[]>`
      SELECT
        rooms.room_id,
        rooms.name
      FROM rooms
      INNER JOIN hook_events ON hook_events.room_id = rooms.room_id
      LEFT JOIN room_summaries ON room_summaries.room_id = rooms.room_id
      GROUP BY rooms.room_id, rooms.name, room_summaries.last_processed_seq
      HAVING MAX(hook_events.seq) > COALESCE(room_summaries.last_processed_seq, 0)
      ORDER BY MAX(hook_events.received_at) ASC, rooms.room_id ASC
      LIMIT ${limit}
    `;

    return rows.map((row) => ({
      room_id: readString(row.room_id, "room_id"),
      name: readString(row.name, "name"),
    }));
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

  private async getStoredRoomSummary(roomId: string): Promise<RoomSnapshot> {
    const [row] = await this.client<Pick<RoomSummaryRow, "content_json">[]>`
      SELECT content_json
      FROM room_summaries
      WHERE room_id = ${normalizeRoomId(roomId)}
    `;

    return normalizeStoredRoomSummary(row?.content_json);
  }

  private async listStoredRoomSummariesForOrganization(
    organizationId: string,
  ): Promise<
    Array<{ room_id: string; snapshot: RoomSnapshot; updated_at: string }>
  > {
    const rows = await this.client<RoomSummaryRow[]>`
      SELECT
        rooms.room_id,
        room_summaries.content_json,
        room_summaries.updated_at
      FROM rooms
      LEFT JOIN room_summaries ON room_summaries.room_id = rooms.room_id
      WHERE rooms.organization_id = ${organizationId}
      ORDER BY rooms.created_at DESC, rooms.room_id DESC
    `;

    return rows.map((row) => ({
      room_id: normalizeRoomId(readString(row.room_id, "room_id")),
      snapshot: normalizeStoredRoomSummary(row.content_json),
      updated_at: toOptionalRfc3339(row.updated_at),
    }));
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

function mapOrganizationMembership(
  row: OrganizationMembershipRow,
): OrganizationMembership {
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
    created_by_user_id: readString(
      row.created_by_user_id,
      "created_by_user_id",
    ),
    bluf_markdown: row.bluf_markdown == null ? "" : String(row.bluf_markdown),
    employee_count:
      row.employee_count == null ? 0 : toNumber(row.employee_count),
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
    bluf_markdown:
      typeof base.bluf_markdown === "string" ? base.bluf_markdown : "",
    rooms: Array.isArray(base.rooms)
      ? base.rooms.map((room) => normalizeOrganizationRoomBlufSnapshot(room))
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

function normalizeRoomSnapshot(
  snapshot: RoomSnapshot | undefined,
): RoomSnapshot {
  const base = snapshot ?? emptyRoomSnapshot();
  return {
    bluf_markdown:
      typeof base.bluf_markdown === "string" ? base.bluf_markdown : "",
    detailed_summary_markdown:
      typeof base.detailed_summary_markdown === "string"
        ? base.detailed_summary_markdown
        : "",
    employees: Array.isArray(base.employees)
      ? base.employees.map(normalizeEmployeeSnapshot)
      : [],
  };
}

function normalizeStoredRoomSummary(snapshot?: RoomSnapshot): RoomSnapshot {
  return normalizeRoomSnapshot(snapshot);
}

function normalizeOrganizationRoomBlufSnapshot(
  snapshot: RoomBlufSnapshot,
): RoomBlufSnapshot {
  return {
    room_id: normalizeRoomId(
      typeof snapshot.room_id === "string" ? snapshot.room_id : "",
    ),
    bluf_markdown:
      typeof snapshot.bluf_markdown === "string" ? snapshot.bluf_markdown : "",
    last_update_at:
      typeof snapshot.last_update_at === "string"
        ? snapshot.last_update_at
        : "",
  };
}

function toRoomBlufSnapshot(
  roomId: string,
  snapshot: RoomSnapshot | undefined,
  updatedAt?: unknown,
): RoomBlufSnapshot {
  const normalized = normalizeStoredRoomSummary(snapshot);
  return {
    room_id: normalizeRoomId(roomId),
    bluf_markdown: normalized.bluf_markdown,
    last_update_at: toOptionalRfc3339(updatedAt),
  };
}

function normalizeEmployeeSnapshot(
  snapshot: EmployeeSnapshot,
): EmployeeSnapshot {
  return {
    employee_name:
      typeof snapshot.employee_name === "string" ? snapshot.employee_name : "",
    room_ids: Array.isArray(snapshot.room_ids)
      ? Array.from(
          new Set(
            snapshot.room_ids
              .filter((roomId): roomId is string => typeof roomId === "string")
              .map(normalizeRoomId),
          ),
        )
      : [],
    bluf_markdown:
      typeof snapshot.bluf_markdown === "string" ? snapshot.bluf_markdown : "",
    last_update_at:
      typeof snapshot.last_update_at === "string"
        ? snapshot.last_update_at
        : "",
  };
}

function generateRoomCode(): string {
  let roomCode = "";
  while (roomCode.length < ROOM_CODE_LENGTH) {
    const bytes = crypto.getRandomValues(
      new Uint8Array(ROOM_CODE_LENGTH - roomCode.length),
    );
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
    const timestamp = Date.parse(value);
    if (Number.isNaN(timestamp)) {
      throw new Error(`failed to decode timestamp: ${value}`);
    }
    return new Date(timestamp).toISOString();
  }
  throw new Error(`failed to decode timestamp: ${String(value)}`);
}

function toOptionalRfc3339(value: unknown): string {
  if (value == null) {
    return "";
  }

  try {
    return toRfc3339(value);
  } catch {
    return "";
  }
}

function isUniqueViolation(error: unknown): boolean {
  if (error instanceof Bun.SQL.PostgresError) {
    return error.code === "23505";
  }
  return false;
}
