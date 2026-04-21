import { cors } from "@elysiajs/cors";
import { Elysia, status, t } from "elysia";

import {
  HOOK_WRITE_PERMISSIONS,
  PROJECT_CONNECTION_KEY_CONFIG,
  type SupermanagerAuth,
} from "./auth";
import { trimUrl, type ApiConfig } from "./config";
import { Db } from "./db";
import {
  httpError,
  readRequiredHeader,
  requireProjectAccess,
  requireViewer,
  resolveOrganizationMembership,
} from "./middleware";
import { createMcpRoutes } from "./mcp/routes";
import { renderOrganizationAgentContextExport } from "./organization-agent-context";
import { indexEventById } from "./search/store";
import type { FeedStreamHub } from "./sse";
import { formatError } from "./types";

const DEFAULT_PUBLIC_API_URL = "https://api.supermanager.dev";
const FEED_PAGE_DEFAULT = 10;
const FEED_PAGE_MAX = 100;
const ACTIVITY_PAGE_DEFAULT = 10;
const ACTIVITY_PAGE_MAX = 50;

const projectParams = t.Object({
  projectId: t.String(),
});

const organizationParams = t.Object({
  organizationSlug: t.String(),
});

const organizationMemberParams = t.Object({
  memberUserId: t.String(),
  organizationSlug: t.String(),
});

const createProjectBody = t.Object({
  name: t.String(),
  organization_slug: t.Optional(t.Nullable(t.String())),
});

const createConnectionBody = t.Object({
  name: t.Optional(t.Nullable(t.String())),
  repo_root: t.String(),
});

const listProjectsQuery = t.Object({
  organization_slug: t.Optional(t.String()),
});

const feedQuery = t.Object({
  before: t.Optional(t.Numeric()),
  limit: t.Optional(t.Numeric()),
});

const activityQuery = t.Object({
  limit: t.Optional(t.Numeric()),
});

const feedStreamHeaders = t.Object({
  "last-event-id": t.Optional(t.Numeric()),
});

const uploadedTranscriptBody = t.Object({
  session_id: t.String(),
  transcript_path: t.String(),
  content_text: t.String(),
});

const hookTurnBody = t.Object({
  client: t.String(),
  repo_root: t.String(),
  branch: t.Optional(t.Nullable(t.String())),
  payload: t.Any(),
  transcript: t.Optional(t.Nullable(uploadedTranscriptBody)),
});

export interface AppContext {
  auth: SupermanagerAuth;
  config: ApiConfig;
  db: Db;
  feedHub: FeedStreamHub;
}

export function createApp(context: AppContext) {
  const allowedOrigins = [
    trimUrl(context.config.publicApiUrl),
    trimUrl(context.config.publicAppUrl),
  ];

  return new Elysia()
    .use(
      cors({
        allowedHeaders: [
          "Accept",
          "Authorization",
          "Content-Type",
          "Last-Event-ID",
          "MCP-Protocol-Version",
          "MCP-Session-Id",
          "X-API-Key",
        ],
        credentials: true,
        exposeHeaders: [
          "mcp-protocol-version",
          "mcp-session-id",
          "set-auth-token",
        ],
        methods: ["DELETE", "GET", "POST", "OPTIONS"],
        origin: allowedOrigins,
      }),
    )
    .mount(context.auth.handler)
    .use(createMcpRoutes(context))
    .onError(({ code, error }) => {
      if (code === "NOT_FOUND") {
        return new Response("not found", { status: 404 });
      }

      const message = error instanceof Error ? error.message : String(error);
      const statusCode =
        typeof (error as { status?: unknown })?.status === "number"
          ? (error as { status: number }).status
          : 500;

      console.error(error);
      return new Response(message, {
        status: statusCode,
        headers: {
          "content-type": "text/plain; charset=utf-8",
        },
      });
    })
    .get("/health", async () => {
      try {
        await context.db.ping();
        return "ok";
      } catch (error) {
        console.error(error);
        return status(503, formatError(error));
      }
    })
    .get("/v1/me", async ({ request }) => {
      const viewer = await requireViewer(context.auth, request.headers);
      const [organizations, hasCliAuth] = await Promise.all([
        context.db.listOrganizationsForUser(viewer.user.id),
        context.db.hasCliAuth(viewer.user.id),
      ]);

      return {
        active_organization_id: viewer.session.activeOrganizationId ?? null,
        has_cli_auth: hasCliAuth,
        organizations,
        user: {
          email: viewer.user.email,
          id: viewer.user.id,
          image: viewer.user.image ?? null,
          name: normalizeDisplayName(viewer.user.name, viewer.user.email),
        },
      };
    })
    .get(
      "/v1/projects",
      async ({ query, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const membership = await resolveOrganizationMembership(
          context.db,
          viewer.user.id,
          query.organization_slug,
          viewer.session.activeOrganizationId ?? null,
        );

        return {
          organization_slug: membership.organization_slug,
          projects: await context.db.listProjectsForOrganization(
            membership.organization_id,
          ),
        };
      },
      {
        query: listProjectsQuery,
      },
    )
    .post(
      "/v1/projects",
      async ({ body, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const name = body.name.trim();
        if (!name) {
          return status(400, "name must be a non-empty string");
        }

        const membership = await resolveOrganizationMembership(
          context.db,
          viewer.user.id,
          body.organization_slug ?? undefined,
          viewer.session.activeOrganizationId ?? null,
        );
        const project = await context.db.createProject(
          membership.organization_id,
          viewer.user.id,
          name,
        );

        return status(201, {
          project_id: project.project_id,
          organization_slug: membership.organization_slug,
          dashboard_url: dashboardUrl(
            context.config.publicAppUrl,
            project.project_id,
          ),
          join_command: cliJoinCommand(
            context.config.publicApiUrl,
            project.project_id,
            membership.organization_slug,
          ),
        });
      },
      {
        body: createProjectBody,
      },
    )
    .get(
      "/v1/organizations/:organizationSlug/agent-context",
      async ({ params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const membership = await resolveOrganizationMembership(
          context.db,
          viewer.user.id,
          params.organizationSlug,
          viewer.session.activeOrganizationId ?? null,
        );
        const [memories, skills] = await Promise.all([
          context.db.getOrganizationWorkflowDocumentsResponse(
            membership.organization_id,
            "organization_memories",
          ),
          context.db.getOrganizationWorkflowDocumentsResponse(
            membership.organization_id,
            "organization_skills",
          ),
        ]);

        return renderOrganizationAgentContextExport({
          organizationSlug: membership.organization_slug,
          memories,
          skills,
        });
      },
      {
        params: organizationParams,
      },
    )
    .get(
      "/v1/organizations/:organizationSlug/summary",
      async ({ params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const membership = await resolveOrganizationMembership(
          context.db,
          viewer.user.id,
          params.organizationSlug,
          viewer.session.activeOrganizationId ?? null,
        );

        return context.db.getOrganizationSummaryResponse(
          membership.organization_id,
        );
      },
      {
        params: organizationParams,
      },
    )
    .get(
      "/v1/organizations/:organizationSlug/updates",
      async ({ params, query, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const membership = await resolveOrganizationMembership(
          context.db,
          viewer.user.id,
          params.organizationSlug,
          viewer.session.activeOrganizationId ?? null,
        );

        return {
          updates: await context.db.getOrganizationUpdates(
            membership.organization_id,
            clampLimit(query.limit, ACTIVITY_PAGE_DEFAULT, ACTIVITY_PAGE_MAX),
          ),
        };
      },
      {
        params: organizationParams,
        query: activityQuery,
      },
    )
    .get(
      "/v1/organizations/:organizationSlug/members/:memberUserId/updates",
      async ({ params, query, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const membership = await resolveOrganizationMembership(
          context.db,
          viewer.user.id,
          params.organizationSlug,
          viewer.session.activeOrganizationId ?? null,
        );

        return {
          updates: await context.db.getMemberUpdates(
            membership.organization_id,
            params.memberUserId.trim(),
            clampLimit(query.limit, ACTIVITY_PAGE_DEFAULT, ACTIVITY_PAGE_MAX),
          ),
        };
      },
      {
        params: organizationMemberParams,
        query: activityQuery,
      },
    )
    .get(
      "/v1/organizations/:organizationSlug/memories",
      async ({ params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const membership = await resolveOrganizationMembership(
          context.db,
          viewer.user.id,
          params.organizationSlug,
          viewer.session.activeOrganizationId ?? null,
        );

        return context.db.getOrganizationWorkflowDocumentsResponse(
          membership.organization_id,
          "organization_memories",
        );
      },
      {
        params: organizationParams,
      },
    )
    .get(
      "/v1/organizations/:organizationSlug/skills",
      async ({ params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const membership = await resolveOrganizationMembership(
          context.db,
          viewer.user.id,
          params.organizationSlug,
          viewer.session.activeOrganizationId ?? null,
        );

        return context.db.getOrganizationWorkflowDocumentsResponse(
          membership.organization_id,
          "organization_skills",
        );
      },
      {
        params: organizationParams,
      },
    )
    .get(
      "/v1/projects/:projectId",
      async ({ params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const project = await requireProjectAccess(
          context.db,
          viewer.user.id,
          params.projectId,
        );

        return {
          project_id: project.project_id,
          name: project.name,
          created_at: project.created_at,
          organization_slug: project.organization_slug,
          join_command: cliJoinCommand(
            context.config.publicApiUrl,
            project.project_id,
            project.organization_slug,
          ),
        };
      },
      {
        params: projectParams,
      },
    )
    .delete(
      "/v1/projects/:projectId",
      async ({ params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const project = await requireProjectAccess(
          context.db,
          viewer.user.id,
          params.projectId,
        );
        const deleted = await context.db.deleteProject(project.project_id);

        if (!deleted) {
          throw httpError(404, `project not found: ${project.project_id}`);
        }

        return new Response(null, { status: 204 });
      },
      {
        params: projectParams,
      },
    )
    .get(
      "/v1/projects/:projectId/feed",
      async ({ params, query, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const project = await requireProjectAccess(
          context.db,
          viewer.user.id,
          params.projectId,
        );

        const before = query.before;
        const limit = clampLimit(query.limit);
        const [events, totalCount] = await Promise.all([
          context.db.getHookEvents(
            project.project_id,
            before,
            undefined,
            limit,
          ),
          context.db.countHookEvents(project.project_id),
        ]);
        return { events, total_count: totalCount };
      },
      {
        params: projectParams,
        query: feedQuery,
      },
    )
    .get(
      "/v1/projects/:projectId/updates",
      async ({ params, query, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const project = await requireProjectAccess(
          context.db,
          viewer.user.id,
          params.projectId,
        );

        return {
          updates: await context.db.getProjectUpdates(
            project.project_id,
            clampLimit(query.limit, ACTIVITY_PAGE_DEFAULT, ACTIVITY_PAGE_MAX),
          ),
        };
      },
      {
        params: projectParams,
        query: activityQuery,
      },
    )
    .get(
      "/v1/projects/:projectId/feed/stream",
      async ({ headers, params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const project = await requireProjectAccess(
          context.db,
          viewer.user.id,
          params.projectId,
        );

        const replay =
          headers["last-event-id"] == null
            ? []
            : await context.db.getHookEvents(
                project.project_id,
                undefined,
                headers["last-event-id"],
                undefined,
              );

        replay.reverse();
        const client = context.feedHub.register(
          project.project_id,
          request.headers.get("origin"),
        );

        for (const event of replay) {
          client.sendHookEvent(event);
        }

        return client.response;
      },
      {
        headers: feedStreamHeaders,
        params: projectParams,
      },
    )
    .post(
      "/v1/projects/:projectId/connections",
      async ({ body, params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const project = await requireProjectAccess(
          context.db,
          viewer.user.id,
          params.projectId,
        );

        const repoRoot = body.repo_root.trim();
        if (!repoRoot) {
          return status(400, "repo_root must be a non-empty string");
        }

        const created = await context.auth.api.createApiKey({
          headers: request.headers,
          body: {
            configId: PROJECT_CONNECTION_KEY_CONFIG,
            metadata: {
              repoRoot,
              projectId: project.project_id,
            },
            name: buildConnectionName(body.name, repoRoot),
          },
        });

        return status(201, {
          api_key: created.key,
          api_key_id: created.id,
          dashboard_url: dashboardUrl(
            context.config.publicAppUrl,
            project.project_id,
          ),
          project_id: project.project_id,
        });
      },
      {
        body: createConnectionBody,
        params: projectParams,
      },
    )
    .post(
      "/v1/hooks/turn",
      async ({ body, request }) => {
        const verification = await context.auth.api.verifyApiKey({
          body: {
            configId: PROJECT_CONNECTION_KEY_CONFIG,
            key: readRequiredHeader(request.headers, "x-api-key"),
            permissions: HOOK_WRITE_PERMISSIONS,
          },
        });

        if (!verification.valid || !verification.key) {
          throw httpError(401, "invalid api key");
        }

        const metadata = parseConnectionMetadata(verification.key.metadata);
        const memberUserId = verification.key.referenceId.trim();
        if (!memberUserId) {
          throw httpError(401, "api key user is invalid");
        }
        const hookTarget = await context.db.getHookEventWriteContext(
          metadata.projectId,
          memberUserId,
        );
        if (!hookTarget.project) {
          throw httpError(404, `project not found: ${metadata.projectId}`);
        }
        if (!hookTarget.userExists) {
          throw httpError(401, "api key user is invalid");
        }
        if (!hookTarget.hasAccess) {
          throw httpError(403, "api key user no longer has project access");
        }
        const project = hookTarget.project;
        const memberName = hookTarget.memberName ?? "Unknown member";

        const client = body.client.trim();
        if (!client) {
          return status(400, "client must be a non-empty string");
        }

        const repoRoot = body.repo_root.trim();
        if (!repoRoot) {
          return status(400, "repo_root must be a non-empty string");
        }
        if (repoRoot !== metadata.repoRoot) {
          throw httpError(403, "api key repo mismatch");
        }

        const stored = await context.db.insertHookEvent(project.project_id, {
          member_user_id: memberUserId,
          member_name: memberName,
          client,
          repo_root: repoRoot,
          branch: body.branch ?? null,
          payload: body.payload,
          transcript: body.transcript ?? null,
        });

        context.feedHub.publishHookEvent(project.project_id, stored);
        void indexEventById(context.db, stored.event_id).catch((error) => {
          console.error(
            `[search] failed to index hook event ${stored.event_id}: ${formatError(error)}`,
          );
        });

        return status(202, {
          event_id: stored.event_id,
          received_at: stored.received_at,
        });
      },
      {
        body: hookTurnBody,
      },
    )
    .get(
      "/v1/projects/:projectId/summary",
      async ({ params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const project = await requireProjectAccess(
          context.db,
          viewer.user.id,
          params.projectId,
        );

        return context.db.getProjectSummaryResponse(project.project_id);
      },
      {
        params: projectParams,
      },
    );
}

function clampLimit(
  value: number | undefined,
  defaultLimit: number = FEED_PAGE_DEFAULT,
  maxLimit: number = FEED_PAGE_MAX,
): number {
  const limit = value ?? defaultLimit;
  return Math.min(Math.max(limit, 1), maxLimit);
}

function dashboardUrl(appUrl: string, projectId: string): string {
  return `${trimUrl(appUrl)}/p/${projectId}`;
}

function cliJoinCommand(
  apiUrl: string,
  projectId: string,
  organizationSlug: string,
): string {
  const normalizedApiUrl = trimUrl(apiUrl);
  const parts = [
    "supermanager",
    "join",
    projectId,
    "--org",
    shellQuote(organizationSlug),
  ];

  if (normalizedApiUrl !== DEFAULT_PUBLIC_API_URL) {
    parts.push("--server", shellQuote(normalizedApiUrl));
  }

  return parts.join(" ");
}

function parseConnectionMetadata(metadata: unknown): {
  repoRoot: string;
  projectId: string;
} {
  if (
    metadata &&
    typeof metadata === "object" &&
    "repoRoot" in metadata &&
    "projectId" in metadata
  ) {
    const projectId = (metadata as { projectId?: unknown }).projectId;
    const repoRoot = (metadata as { repoRoot?: unknown }).repoRoot;
    if (
      typeof projectId === "string" &&
      projectId.trim() &&
      typeof repoRoot === "string" &&
      repoRoot.trim()
    ) {
      return { repoRoot, projectId };
    }
  }

  throw httpError(401, "api key metadata is invalid");
}

function buildConnectionName(
  name: string | null | undefined,
  repoRoot: string,
) {
  const trimmedName = name?.trim();
  if (trimmedName) {
    return trimmedName;
  }

  return repoRoot.split(/[\\/]/).filter(Boolean).at(-1) ?? repoRoot;
}

function normalizeDisplayName(name: string | null | undefined, email: string) {
  return name?.trim() || email;
}

function shellQuote(value: string): string {
  return `"${value.replaceAll('"', '\\"')}"`;
}
