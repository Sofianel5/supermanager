import { cors } from "@elysiajs/cors";
import { Elysia, status, t } from "elysia";

import type { ServerConfig } from "./config";
import { Db } from "./db";
import type { FeedStreamHub } from "./sse";
import type { StoragePaths } from "./storage";
import type { SummaryAgentHost } from "./summary/agent-host";

const DEFAULT_PUBLIC_API_URL = "https://api.supermanager.dev";
const DEFAULT_PUBLIC_APP_URL = "https://supermanager.dev";
const FEED_PAGE_DEFAULT = 10;
const FEED_PAGE_MAX = 100;

const roomParams = t.Object({
  roomId: t.String(),
});

const createRoomBody = t.Object({
  name: t.String(),
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
  config: ServerConfig;
  db: Db;
  storage: StoragePaths;
  agent: SummaryAgentHost;
  feedHub: FeedStreamHub;
}

export function createApp(context: AppContext) {
  return new Elysia()
    .use(
      cors({
        allowedHeaders: ["Content-Type", "Last-Event-ID"],
        methods: ["GET", "POST", "OPTIONS"],
        origin: true,
      }),
    )
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
        await context.storage.checkReady();
        await context.agent.checkReady();
        return "ok";
      } catch (error) {
        console.error(error);
        return status(503, formatError(error));
      }
    })
    .post(
      "/v1/rooms",
      async ({ body }) => {
        const name = body.name.trim();
        if (!name) {
          return status(400, "name must be a non-empty string");
        }

        const room = await context.db.createRoom(name);
        return status(201, {
          room_id: room.room_id,
          dashboard_url: dashboardUrl(context.config.publicAppUrl, room.room_id),
          join_command: cliJoinCommand(
            context.config.publicApiUrl,
            context.config.publicAppUrl,
            room.room_id,
          ),
        });
      },
      {
        body: createRoomBody,
      },
    )
    .get(
      "/r/:roomId",
      async ({ params }) => {
        const room = await context.db.getRoom(params.roomId);
        if (!room) {
          return status(404, `room not found: ${params.roomId}`);
        }

        return {
          room_id: room.room_id,
          name: room.name,
          created_at: room.created_at,
        };
      },
      {
        params: roomParams,
      },
    )
    .get(
      "/r/:roomId/feed",
      async ({ params, query }) => {
        const room = await context.db.getRoom(params.roomId);
        if (!room) {
          return status(404, `room not found: ${params.roomId}`);
        }

        const before = query.before;
        const limit = clampLimit(query.limit);
        const events = await context.db.getHookEvents(room.room_id, before, undefined, limit);
        return { events };
      },
      {
        params: roomParams,
        query: feedQuery,
      },
    )
    .get(
      "/r/:roomId/feed/stream",
      async ({ headers, params }) => {
        const room = await context.db.getRoom(params.roomId);
        if (!room) {
          return status(404, `room not found: ${params.roomId}`);
        }

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
        const initialStatus = await context.db.getSummaryStatus(room.room_id);
        const client = context.feedHub.register(room.room_id);

        for (const event of replay) {
          client.sendHookEvent(event);
        }
        client.sendSummaryStatus(initialStatus);

        return client.response;
      },
      {
        headers: feedStreamHeaders,
        params: roomParams,
      },
    )
    .post(
      "/r/:roomId/hooks/turn",
      async ({ body, params }) => {
        const room = await context.db.getRoom(params.roomId);
        if (!room) {
          return status(404, `room not found: ${params.roomId}`);
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

        const stored = await context.db.insertHookEvent(room.room_id, {
          employee_name: employeeName,
          client,
          repo_root: repoRoot,
          branch: body.branch ?? null,
          payload: body.payload,
        });

        context.feedHub.publishHookEvent(room.room_id, stored);
        await context.agent.enqueue(room.room_id, stored);

        return status(202, {
          event_id: stored.event_id,
          received_at: stored.received_at,
        });
      },
      {
        body: hookTurnBody,
        params: roomParams,
      },
    )
    .get(
      "/r/:roomId/summary",
      async ({ params }) => {
        const room = await context.db.getRoom(params.roomId);
        if (!room) {
          return status(404, `room not found: ${params.roomId}`);
        }

        return context.db.getSummary(room.room_id);
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

function cliJoinCommand(apiUrl: string, appUrl: string, roomId: string): string {
  const normalizedApiUrl = trimUrl(apiUrl);
  const normalizedAppUrl = trimUrl(appUrl);

  if (
    normalizedApiUrl === DEFAULT_PUBLIC_API_URL &&
    normalizedAppUrl === DEFAULT_PUBLIC_APP_URL
  ) {
    return `supermanager join ${roomId}`;
  }

  return `supermanager join ${roomId} --server "${normalizedApiUrl}" --app-url "${normalizedAppUrl}"`;
}

function trimUrl(url: string): string {
  return url.replace(/\/+$/, "");
}

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
