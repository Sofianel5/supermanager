import type { ServerConfig } from "./config.js";
import { Db } from "./db.js";
import { BadRequestError, NotFoundError, formatError } from "./http/errors.js";
import {
  clampInteger,
  matchRoomPath,
  parseCreateRoomRequest,
  parseHookTurnReport,
  parseLastEventId,
  parseOptionalInteger,
  readJson,
} from "./http/requests.js";
import { jsonResponse, noContentResponse, textResponse } from "./http/responses.js";
import { FeedStreamHub } from "./sse.js";
import { StoragePaths } from "./storage.js";
import { SummaryAgentHost } from "./summary/agent-host.js";
import type { Room } from "./types.js";

const DEFAULT_PUBLIC_API_URL = "https://api.supermanager.dev";
const DEFAULT_PUBLIC_APP_URL = "https://supermanager.dev";
const FEED_PAGE_DEFAULT = 10;
const FEED_PAGE_MAX = 100;

export interface AppContext {
  config: ServerConfig;
  db: Db;
  storage: StoragePaths;
  agent: SummaryAgentHost;
  feedHub: FeedStreamHub;
}

export function createFetchHandler(context: AppContext) {
  return async (request: Request): Promise<Response> => {
    if (request.method === "OPTIONS") {
      return noContentResponse();
    }

    const url = new URL(request.url);
    const pathname = url.pathname;

    try {
      if (request.method === "GET" && pathname === "/health") {
        await context.db.ping();
        await context.storage.checkReady();
        await context.agent.checkReady();
        return textResponse("ok", 200);
      }

      if (request.method === "POST" && pathname === "/v1/rooms") {
        const body = parseCreateRoomRequest(await readJson(request));
        const room = await context.db.createRoom(body.name);
        return jsonResponse(
          {
            room_id: room.room_id,
            dashboard_url: dashboardUrl(context.config.publicAppUrl, room.room_id),
            join_command: cliJoinCommand(
              context.config.publicApiUrl,
              context.config.publicAppUrl,
              room.room_id,
            ),
          },
          201,
        );
      }

      const roomMatch = matchRoomPath(pathname);
      if (roomMatch) {
        const room = await resolveRoom(context.db, roomMatch.roomId);

        if (request.method === "GET" && roomMatch.suffix === "") {
          return jsonResponse({
            room_id: room.room_id,
            name: room.name,
            created_at: room.created_at,
          });
        }

        if (request.method === "GET" && roomMatch.suffix === "/feed") {
          const limit = clampInteger(
            url.searchParams.get("limit") ?? undefined,
            FEED_PAGE_DEFAULT,
            1,
            FEED_PAGE_MAX,
          );
          const before = parseOptionalInteger(
            url.searchParams.get("before") ?? undefined,
            "before",
          );
          const events = await context.db.getHookEvents(room.room_id, before, undefined, limit);
          return jsonResponse({ events });
        }

        if (request.method === "GET" && roomMatch.suffix === "/feed/stream") {
          const lastEventId = parseLastEventId(request.headers.get("last-event-id"));
          const replay =
            lastEventId == null
              ? []
              : await context.db.getHookEvents(room.room_id, undefined, lastEventId, undefined);
          replay.reverse();
          const initialStatus = await context.db.getSummaryStatus(room.room_id);

          const client = context.feedHub.register(room.room_id);
          for (const event of replay) {
            client.sendHookEvent(event);
          }
          client.sendSummaryStatus(initialStatus);
          return client.response;
        }

        if (request.method === "POST" && roomMatch.suffix === "/hooks/turn") {
          const report = parseHookTurnReport(await readJson(request));
          const stored = await context.db.insertHookEvent(room.room_id, report);
          context.feedHub.publishHookEvent(room.room_id, stored);
          await context.agent.enqueue(room.room_id, stored);
          return jsonResponse(
            {
              event_id: stored.event_id,
              received_at: stored.received_at,
            },
            202,
          );
        }

        if (request.method === "GET" && roomMatch.suffix === "/summary") {
          const summary = await context.db.getSummary(room.room_id);
          return jsonResponse(summary);
        }
      }

      return textResponse("not found", 404);
    } catch (error) {
      if (error instanceof BadRequestError) {
        return textResponse(error.message, 400);
      }
      if (error instanceof NotFoundError) {
        return textResponse(error.message, 404);
      }
      console.error(error);
      return textResponse(formatError(error), pathname === "/health" ? 503 : 500);
    }
  };
}

async function resolveRoom(db: Db, roomId: string): Promise<Room> {
  const room = await db.getRoom(roomId);
  if (!room) {
    throw new NotFoundError(`room not found: ${roomId}`);
  }
  return room;
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
