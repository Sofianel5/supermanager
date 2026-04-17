import { cors } from "@elysiajs/cors";
import { Elysia, status, t } from "elysia";

import {
  HOOK_WRITE_PERMISSIONS,
  ROOM_CONNECTION_KEY_CONFIG,
  type SupermanagerAuth,
} from "./auth";
import { trimUrl, type ApiConfig } from "./config";
import { Db } from "./db";
import {
  httpError,
  readRequiredHeader,
  requireRoomAccess,
  requireViewer,
  resolveOrganizationMembership,
} from "./middleware";
import { createMcpRoutes } from "./mcp/routes";
import { indexEventById } from "./search/store";
import type { FeedStreamHub } from "./sse";

const DEFAULT_PUBLIC_API_URL = "https://api.supermanager.dev";
const FEED_PAGE_DEFAULT = 10;
const FEED_PAGE_MAX = 100;

const roomParams = t.Object({
  roomId: t.String(),
});

const organizationParams = t.Object({
  organizationSlug: t.String(),
});

const createRoomBody = t.Object({
  name: t.String(),
  organization_slug: t.Optional(t.Nullable(t.String())),
});

const createConnectionBody = t.Object({
  name: t.Optional(t.Nullable(t.String())),
  repo_root: t.String(),
});

const listRoomsQuery = t.Object({
  organization_slug: t.Optional(t.String()),
});

const feedQuery = t.Object({
  before: t.Optional(t.Numeric()),
  limit: t.Optional(t.Numeric()),
});

const feedStreamHeaders = t.Object({
  "last-event-id": t.Optional(t.Numeric()),
});

const hookTurnBody = t.Object({
  employee_name: t.String(),
  client: t.String(),
  repo_root: t.String(),
  branch: t.Optional(t.Nullable(t.String())),
  payload: t.Any(),
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
      "/v1/rooms",
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
          rooms: await context.db.listRoomsForOrganization(
            membership.organization_id,
          ),
        };
      },
      {
        query: listRoomsQuery,
      },
    )
    .post(
      "/v1/rooms",
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
        const room = await context.db.createRoom(
          membership.organization_id,
          viewer.user.id,
          name,
        );

        return status(201, {
          room_id: room.room_id,
          organization_slug: membership.organization_slug,
          dashboard_url: dashboardUrl(
            context.config.publicAppUrl,
            room.room_id,
          ),
          join_command: cliJoinCommand(
            context.config.publicApiUrl,
            room.room_id,
            membership.organization_slug,
          ),
        });
      },
      {
        body: createRoomBody,
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

        return {
          status: await context.db.getOrganizationSummaryStatus(
            membership.organization_id,
          ),
          summary: await context.db.getOrganizationSummary(
            membership.organization_id,
          ),
        };
      },
      {
        params: organizationParams,
      },
    )
    .get(
      "/v1/rooms/:roomId",
      async ({ params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const room = await requireRoomAccess(
          context.db,
          viewer.user.id,
          params.roomId,
        );

        return {
          room_id: room.room_id,
          name: room.name,
          created_at: room.created_at,
          organization_slug: room.organization_slug,
          join_command: cliJoinCommand(
            context.config.publicApiUrl,
            room.room_id,
            room.organization_slug,
          ),
        };
      },
      {
        params: roomParams,
      },
    )
    .get(
      "/v1/rooms/:roomId/feed",
      async ({ params, query, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const room = await requireRoomAccess(
          context.db,
          viewer.user.id,
          params.roomId,
        );

        const before = query.before;
        const limit = clampLimit(query.limit);
        const [events, totalCount] = await Promise.all([
          context.db.getHookEvents(room.room_id, before, undefined, limit),
          context.db.countHookEvents(room.room_id),
        ]);
        return { events, total_count: totalCount };
      },
      {
        params: roomParams,
        query: feedQuery,
      },
    )
    .get(
      "/v1/rooms/:roomId/feed/stream",
      async ({ headers, params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const room = await requireRoomAccess(
          context.db,
          viewer.user.id,
          params.roomId,
        );

        const replay =
          headers["last-event-id"] == null
            ? []
            : await context.db.getHookEvents(
                room.room_id,
                undefined,
                headers["last-event-id"],
                undefined,
              );

        replay.reverse();
        const client = context.feedHub.register(
          room.room_id,
          request.headers.get("origin"),
        );

        for (const event of replay) {
          client.sendHookEvent(event);
        }

        return client.response;
      },
      {
        headers: feedStreamHeaders,
        params: roomParams,
      },
    )
    .post(
      "/v1/rooms/:roomId/connections",
      async ({ body, params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const room = await requireRoomAccess(
          context.db,
          viewer.user.id,
          params.roomId,
        );

        const repoRoot = body.repo_root.trim();
        if (!repoRoot) {
          return status(400, "repo_root must be a non-empty string");
        }

        const created = await context.auth.api.createApiKey({
          headers: request.headers,
          body: {
            configId: ROOM_CONNECTION_KEY_CONFIG,
            metadata: {
              repoRoot,
              roomId: room.room_id,
            },
            name: buildConnectionName(body.name, repoRoot),
          },
        });

        return status(201, {
          api_key: created.key,
          api_key_id: created.id,
          dashboard_url: dashboardUrl(
            context.config.publicAppUrl,
            room.room_id,
          ),
          room_id: room.room_id,
        });
      },
      {
        body: createConnectionBody,
        params: roomParams,
      },
    )
    .post(
      "/v1/hooks/turn",
      async ({ body, request }) => {
        const verification = await context.auth.api.verifyApiKey({
          body: {
            configId: ROOM_CONNECTION_KEY_CONFIG,
            key: readRequiredHeader(request.headers, "x-api-key"),
            permissions: HOOK_WRITE_PERMISSIONS,
          },
        });

        if (!verification.valid || !verification.key) {
          throw httpError(401, "invalid api key");
        }

        const metadata = parseConnectionMetadata(verification.key.metadata);
        const room = await context.db.getRoom(metadata.roomId);
        if (!room) {
          throw httpError(404, `room not found: ${metadata.roomId}`);
        }

        const employeeName = body.employee_name.trim();
        if (!employeeName) {
          return status(400, "employee_name must be a non-empty string");
        }

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

        const stored = await context.db.insertHookEvent(room.room_id, {
          employee_name: employeeName,
          client,
          repo_root: repoRoot,
          branch: body.branch ?? null,
          payload: body.payload,
        });

        context.feedHub.publishHookEvent(room.room_id, stored);
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
      "/v1/rooms/:roomId/summary",
      async ({ params, request }) => {
        const viewer = await requireViewer(context.auth, request.headers);
        const room = await requireRoomAccess(
          context.db,
          viewer.user.id,
          params.roomId,
        );

        return context.db.getRoomSummaryResponse(room.room_id);
      },
      {
        params: roomParams,
      },
    );
}

function clampLimit(value: number | undefined): number {
  const limit = value ?? FEED_PAGE_DEFAULT;
  return Math.min(Math.max(limit, 1), FEED_PAGE_MAX);
}

function dashboardUrl(appUrl: string, roomId: string): string {
  return `${trimUrl(appUrl)}/r/${roomId}`;
}

function cliJoinCommand(
  apiUrl: string,
  roomId: string,
  organizationSlug: string,
): string {
  const normalizedApiUrl = trimUrl(apiUrl);
  const parts = [
    "supermanager",
    "join",
    roomId,
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
  roomId: string;
} {
  if (
    metadata &&
    typeof metadata === "object" &&
    "repoRoot" in metadata &&
    "roomId" in metadata
  ) {
    const roomId = (metadata as { roomId?: unknown }).roomId;
    const repoRoot = (metadata as { repoRoot?: unknown }).repoRoot;
    if (
      typeof roomId === "string" &&
      roomId.trim() &&
      typeof repoRoot === "string" &&
      repoRoot.trim()
    ) {
      return { repoRoot, roomId };
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

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
