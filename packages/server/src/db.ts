import {
  type EmployeeSnapshot,
  type OrganizationMembership,
  type OrganizationSummaryResponse,
  type OrganizationSnapshot,
  type ProjectListEntry,
  type ProjectBlufSnapshot,
  type ProjectSummaryResponse,
  type ProjectSnapshot,
  type StoredHookEvent,
  type SummaryStatus,
  emptyOrganizationSnapshot,
  emptyProjectSnapshot,
  formatError,
} from "./types";
import { CLI_DEVICE_CLIENT_ID, CLI_USER_AGENT_PREFIX } from "./auth";

const PROJECT_CODE_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const PROJECT_CODE_LENGTH = 6;

interface CreateProjectRow {
  created_at: unknown;
}

interface ProjectRow {
  project_id: unknown;
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
  project_id?: unknown;
  employee_user_id: unknown;
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
  status?: string | null;
  updated_at?: string | Date | null;
}

interface ProjectSummaryRow {
  project_id?: unknown;
  content_json?: ProjectSnapshot;
  last_processed_seq?: unknown;
  status: unknown;
  updated_at?: unknown;
}

interface SummaryStatusRow {
  status: unknown;
}

interface HookEventInsert {
  employee_user_id: string;
  employee_name: string;
  client: string;
  repo_root: string;
  branch: string | null;
  payload: unknown;
}

interface HookEventWriteContextRow extends ProjectRow {
  has_access: unknown;
  user_exists: unknown;
  employee_name: unknown;
}
export interface ProjectRecord extends ProjectListEntry {
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

  async createProject(
    organizationId: string,
    createdByUserId: string,
    name: string,
  ): Promise<{ project_id: string; name: string; created_at: string }> {
    for (let attempt = 0; attempt < 10; attempt += 1) {
      const projectId = generateProjectCode();

      try {
        const [row] = await this.client<CreateProjectRow[]>`
          INSERT INTO projects (project_id, organization_id, created_by_user_id, name)
          VALUES (${projectId}, ${organizationId}, ${createdByUserId}, ${name})
          RETURNING created_at
        `;

        return {
          project_id: projectId,
          name,
          created_at: toRfc3339(row?.created_at),
        };
      } catch (error) {
        if (isUniqueViolation(error)) {
          continue;
        }
        throw new Error(
          `failed to insert project into PostgreSQL: ${formatError(error)}`,
        );
      }
    }

    throw new Error("failed to generate unique project code after 10 attempts");
  }

  async listProjectsForOrganization(
    organizationId: string,
  ): Promise<ProjectListEntry[]> {
    const [rows, projectSummaries] = await Promise.all([
      this.client<ProjectRow[]>`
        SELECT
          projects.project_id,
          projects.name,
          projects.created_at,
          projects.organization_id,
          organization.slug AS organization_slug,
          projects.created_by_user_id
        FROM projects
        INNER JOIN organization ON organization.id = projects.organization_id
        WHERE projects.organization_id = ${organizationId}
        ORDER BY projects.created_at DESC, projects.project_id DESC
      `,
      this.listStoredProjectSummariesForOrganization(organizationId),
    ]);

    const projectBlufMap = new Map(
      projectSummaries.map((project) => [project.project_id, project.snapshot.bluf_markdown]),
    );
    const employeeCounts = new Map(
      projectSummaries.map((project) => [
        project.project_id,
        project.snapshot.employees.length,
      ]),
    );

    return rows.map((row) => {
      const project = mapProject(row);
      return {
        ...project,
        bluf_markdown: projectBlufMap.get(project.project_id) ?? "",
        employee_count: employeeCounts.get(project.project_id) ?? 0,
      };
    });
  }

  async getProjectWithAccessCheck(
    projectId: string,
    userId: string,
  ): Promise<ProjectRecord | null> {
    const [row] = await this.client<ProjectRow[]>`
      SELECT
        projects.project_id,
        projects.name,
        projects.created_at,
        projects.organization_id,
        organization.slug AS organization_slug,
        projects.created_by_user_id
      FROM projects
      INNER JOIN organization ON organization.id = projects.organization_id
      INNER JOIN member ON member."organizationId" = projects.organization_id AND member."userId" = ${userId}
      WHERE projects.project_id = ${normalizeProjectId(projectId)}
    `;

    return row ? mapProject(row) : null;
  }

  async getProject(projectId: string): Promise<ProjectRecord | null> {
    const [row] = await this.client<ProjectRow[]>`
      SELECT
        projects.project_id,
        projects.name,
        projects.created_at,
        projects.organization_id,
        organization.slug AS organization_slug,
        projects.created_by_user_id
      FROM projects
      INNER JOIN organization ON organization.id = projects.organization_id
      WHERE projects.project_id = ${normalizeProjectId(projectId)}
    `;

    return row ? mapProject(row) : null;
  }

  async getHookEventWriteContext(
    projectId: string,
    userId: string,
  ): Promise<{
    project: ProjectRecord | null;
    hasAccess: boolean;
    userExists: boolean;
    employeeName: string | null;
  }> {
    const [row] = await this.client<HookEventWriteContextRow[]>`
      SELECT
        projects.project_id,
        projects.name,
        projects.created_at,
        projects.organization_id,
        organization.slug AS organization_slug,
        projects.created_by_user_id,
        (member."userId" IS NOT NULL) AS has_access,
        ("user".id IS NOT NULL) AS user_exists,
        COALESCE(
          NULLIF(BTRIM("user".name), ''),
          NULLIF(BTRIM("user".email), '')
        ) AS employee_name
      FROM projects
      INNER JOIN organization ON organization.id = projects.organization_id
      LEFT JOIN member ON member."organizationId" = projects.organization_id AND member."userId" = ${userId}
      LEFT JOIN "user" ON "user".id = ${userId}
      WHERE projects.project_id = ${normalizeProjectId(projectId)}
    `;

    return {
      project: row ? mapProject(row) : null,
      hasAccess: row ? readBoolean(row.has_access, "has_access") : false,
      userExists: row ? readBoolean(row.user_exists, "user_exists") : false,
      employeeName:
        row?.employee_name == null
          ? null
          : readString(row.employee_name, "employee_name"),
    };
  }

  async getUserDisplayNames(userIds: string[]): Promise<Map<string, string>> {
    const normalizedUserIds = Array.from(
      new Set(userIds.map((userId) => userId.trim()).filter(Boolean)),
    );
    if (normalizedUserIds.length === 0) {
      return new Map();
    }

    const rows = await this.client<
      Array<{ user_id: unknown; employee_name: unknown }>
    >`
      SELECT
        id AS user_id,
        COALESCE(NULLIF(BTRIM(name), ''), email) AS employee_name
      FROM "user"
      WHERE id = ANY(${normalizedUserIds}::text[])
    `;

    return new Map(
      rows.map((row) => [
        readString(row.user_id, "user_id"),
        readString(row.employee_name, "employee_name"),
      ]),
    );
  }

  async insertHookEvent(
    projectId: string,
    report: HookEventInsert,
  ): Promise<StoredHookEvent> {
    const normalizedProjectId = normalizeProjectId(projectId);
    const eventId = crypto.randomUUID();
    const [row] = await this.client<InsertHookEventRow[]>`
      INSERT INTO hook_events (
        event_id,
        project_id,
        employee_user_id,
        employee_name,
        client,
        repo_root,
        branch,
        payload_json
      )
      VALUES (
        ${eventId},
        ${normalizedProjectId},
        ${report.employee_user_id},
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
      employee_user_id: report.employee_user_id,
      employee_name: report.employee_name,
      client: report.client,
      repo_root: report.repo_root,
      branch: report.branch,
      payload: report.payload,
    };
  }

  async getHookEvents(
    projectId: string,
    before: number | undefined,
    after: number | undefined,
    limit: number | undefined,
  ): Promise<StoredHookEvent[]> {
    const effectiveLimit = limit ?? Number.MAX_SAFE_INTEGER;
    const rows = await this.client<HookEventRow[]>`
      SELECT
        h.seq,
        h.event_id,
        h.employee_user_id,
        COALESCE(NULLIF(BTRIM(u.name), ''), u.email, h.employee_name) AS employee_name,
        h.client,
        h.repo_root,
        h.branch,
        h.payload_json,
        h.received_at
      FROM hook_events AS h
      LEFT JOIN "user" AS u ON u.id = h.employee_user_id
      WHERE h.project_id = ${normalizeProjectId(projectId)}
        AND (${before ?? null}::bigint IS NULL OR h.seq < ${before ?? null})
        AND (${after ?? null}::bigint IS NULL OR h.seq > ${after ?? null})
      ORDER BY h.seq DESC
      LIMIT ${effectiveLimit}
    `;

    return rows.map(mapStoredHookEvent);
  }

  async countHookEvents(projectId: string): Promise<number> {
    const [row] = await this.client<CountRow[]>`
      SELECT COUNT(*)::INT AS count
      FROM hook_events
      WHERE project_id = ${normalizeProjectId(projectId)}
    `;

    return row == null ? 0 : toNumber(row.count);
  }

  async getOrganizationSummaryResponse(
    organizationId: string,
  ): Promise<OrganizationSummaryResponse> {
    const [row, projects] = await Promise.all([
      this.getStoredOrganizationSummaryRow(organizationId),
      this.listProjectBlufsForOrganization(organizationId),
    ]);
    const snapshot = normalizeStoredOrganizationSummary(row?.content_json);

    return {
      status: row ? parseSummaryStatus(row.status) : "ready",
      updated_at: toOptionalRfc3339(row?.updated_at),
      summary: {
        ...(await this.resolveSnapshotEmployeeNames(snapshot)),
        projects,
      },
    };
  }

  async getProjectSummary(projectId: string): Promise<ProjectSnapshot> {
    return this.resolveSnapshotEmployeeNames(
      await this.getStoredProjectSummary(normalizeProjectId(projectId)),
    );
  }

  async getProjectSummaryResponse(projectId: string): Promise<ProjectSummaryResponse> {
    const normalizedProjectId = normalizeProjectId(projectId);
    const [row] = await this.client<ProjectSummaryRow[]>`
      SELECT content_json, last_processed_seq, status
      FROM project_summaries
      WHERE project_id = ${normalizedProjectId}
    `;

    return {
      last_processed_seq:
        row?.last_processed_seq == null ? 0 : toNumber(row.last_processed_seq),
      status: row ? parseSummaryStatus(row.status) : "ready",
      summary: await this.resolveSnapshotEmployeeNames(
        normalizeStoredProjectSummary(row?.content_json),
      ),
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

  async listProjectBlufsForOrganization(
    organizationId: string,
  ): Promise<ProjectBlufSnapshot[]> {
    const rows =
      await this.listStoredProjectSummariesForOrganization(organizationId);
    return rows.map((row) =>
      toProjectBlufSnapshot(row.project_id, row.snapshot, row.updated_at),
    );
  }

  async getProjectBlufSnapshot(projectId: string): Promise<ProjectBlufSnapshot> {
    const normalizedProjectId = normalizeProjectId(projectId);
    const [row] = await this.client<
      Pick<ProjectSummaryRow, "content_json" | "updated_at">[]
    >`
      SELECT content_json, updated_at
      FROM project_summaries
      WHERE project_id = ${normalizedProjectId}
    `;

    return toProjectBlufSnapshot(
      normalizedProjectId,
      row?.content_json,
      row?.updated_at,
    );
  }

  private async getStoredOrganizationSummaryRow(
    organizationId: string,
  ): Promise<SummaryRow | undefined> {
    const [row] = await this.client<SummaryRow[]>`
      SELECT content_json, status, updated_at
      FROM organization_summaries
      WHERE organization_id = ${organizationId}
    `;
    return row;
  }

  private async getStoredProjectSummary(projectId: string): Promise<ProjectSnapshot> {
    const [row] = await this.client<Pick<ProjectSummaryRow, "content_json">[]>`
      SELECT content_json
      FROM project_summaries
      WHERE project_id = ${normalizeProjectId(projectId)}
    `;

    return normalizeStoredProjectSummary(row?.content_json);
  }

  private async listStoredProjectSummariesForOrganization(
    organizationId: string,
  ): Promise<
    Array<{ project_id: string; snapshot: ProjectSnapshot; updated_at: string }>
  > {
    const rows = await this.client<ProjectSummaryRow[]>`
      SELECT
        projects.project_id,
        project_summaries.content_json,
        project_summaries.updated_at
      FROM projects
      LEFT JOIN project_summaries ON project_summaries.project_id = projects.project_id
      WHERE projects.organization_id = ${organizationId}
      ORDER BY projects.created_at DESC, projects.project_id DESC
    `;

    return rows.map((row) => ({
      project_id: normalizeProjectId(readString(row.project_id, "project_id")),
      snapshot: normalizeStoredProjectSummary(row.content_json),
      updated_at: toOptionalRfc3339(row.updated_at),
    }));
  }

  private async resolveSnapshotEmployeeNames<T extends OrganizationSnapshot | ProjectSnapshot>(
    snapshot: T,
  ): Promise<T> {
    const userNames = await this.getUserDisplayNames(
      snapshot.employees.map((employee) => employee.employee_user_id),
    );

    return {
      ...snapshot,
      employees: snapshot.employees.map((employee) => ({
        ...employee,
        employee_name:
          userNames.get(employee.employee_user_id) ?? employee.employee_name,
      })),
    };
  }
}

export function normalizeProjectId(projectId: string): string {
  return projectId.trim().toUpperCase();
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

function mapProject(row: ProjectRow): ProjectRecord {
  return {
    project_id: readString(row.project_id, "project_id"),
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
    employee_user_id: readString(row.employee_user_id, "employee_user_id"),
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
    projects: Array.isArray(base.projects)
      ? base.projects.map((project) => normalizeOrganizationProjectBlufSnapshot(project))
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
    projects: [],
  };
}

function normalizeProjectSnapshot(
  snapshot: ProjectSnapshot | undefined,
): ProjectSnapshot {
  const base = snapshot ?? emptyProjectSnapshot();
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

function normalizeStoredProjectSummary(snapshot?: ProjectSnapshot): ProjectSnapshot {
  return normalizeProjectSnapshot(snapshot);
}

function normalizeOrganizationProjectBlufSnapshot(
  snapshot: ProjectBlufSnapshot,
): ProjectBlufSnapshot {
  return {
    project_id: normalizeProjectId(
      typeof snapshot.project_id === "string" ? snapshot.project_id : "",
    ),
    bluf_markdown:
      typeof snapshot.bluf_markdown === "string" ? snapshot.bluf_markdown : "",
    last_update_at:
      typeof snapshot.last_update_at === "string"
        ? snapshot.last_update_at
        : "",
  };
}

function toProjectBlufSnapshot(
  projectId: string,
  snapshot: ProjectSnapshot | undefined,
  updatedAt?: unknown,
): ProjectBlufSnapshot {
  const normalized = normalizeStoredProjectSummary(snapshot);
  return {
    project_id: normalizeProjectId(projectId),
    bluf_markdown: normalized.bluf_markdown,
    last_update_at: toOptionalRfc3339(updatedAt),
  };
}

function normalizeEmployeeSnapshot(snapshot: EmployeeSnapshot): EmployeeSnapshot {
  const employeeUserId = snapshot.employee_user_id.trim();
  return {
    employee_user_id: employeeUserId,
    employee_name:
      typeof snapshot.employee_name === "string" ? snapshot.employee_name : "",
    project_ids: Array.isArray(snapshot.project_ids)
      ? Array.from(
          new Set(
            snapshot.project_ids
              .filter((projectId): projectId is string => typeof projectId === "string")
              .map(normalizeProjectId),
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

function generateProjectCode(): string {
  let projectCode = "";
  while (projectCode.length < PROJECT_CODE_LENGTH) {
    const bytes = crypto.getRandomValues(
      new Uint8Array(PROJECT_CODE_LENGTH - projectCode.length),
    );
    for (const byte of bytes) {
      if (byte >= 252) {
        continue;
      }
      projectCode += PROJECT_CODE_ALPHABET[byte % PROJECT_CODE_ALPHABET.length];
      if (projectCode.length === PROJECT_CODE_LENGTH) {
        break;
      }
    }
  }
  return projectCode;
}

function readString(value: unknown, key: string): string {
  if (typeof value !== "string") {
    throw new Error(`failed to decode ${key}`);
  }
  return value;
}

function readBoolean(value: unknown, key: string): boolean {
  if (typeof value !== "boolean") {
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
