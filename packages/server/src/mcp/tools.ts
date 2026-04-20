import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import * as z from "zod/v4";

import type { SupermanagerAuth } from "../auth";
import type { ApiConfig } from "../config";
import type { Db } from "../db";
import {
  requireRoomAccess,
  requireViewer,
  resolveOrganizationMembership,
} from "../middleware";
import {
  queryOrganizationEvents,
  searchEvents,
  type SearchEventResult,
} from "../search/store";

const SERVER_INFO = {
  name: "supermanager",
  title: "Supermanager",
  version: "0.1.0",
} as const;

const SERVER_INSTRUCTIONS =
  "Use get_organization_summary for the org-level snapshot, get_room_summary for a single room view, and query_events or search_events for raw historical evidence.";

const DEFAULT_FEED_LIMIT = 25;
const DEFAULT_QUERY_LIMIT = 25;
const DEFAULT_SEARCH_LIMIT = 10;
const MAX_FEED_LIMIT = 100;
const MAX_QUERY_LIMIT = 100;
const MAX_SEARCH_LIMIT = 25;

const READ_ONLY_TOOL = {
  readOnlyHint: true,
  destructiveHint: false,
  idempotentHint: true,
  openWorldHint: false,
} as const;

const listRoomsSchema = z.object({
  organization_slug: optionalString(
    "Optional organization slug. Defaults to the active organization.",
  ),
});

const getRoomSummarySchema = z.object({
  room_id: requiredString("The room identifier."),
});

const getOrganizationSummarySchema = z.object({
  organization_slug: optionalString(
    "Optional organization slug. Defaults to the active organization.",
  ),
});

const getRoomFeedSchema = z.object({
  room_id: requiredString("The room identifier."),
  before_seq: z
    .number()
    .int()
    .min(1)
    .describe("Optional sequence cursor for pagination.")
    .optional(),
  limit: boundedLimit(
    DEFAULT_FEED_LIMIT,
    MAX_FEED_LIMIT,
    "Optional number of events to return.",
  ),
});

const eventQueryFields = {
  organization_slug: optionalString(
    "Optional organization slug. Defaults to the active organization.",
  ),
  room_id: optionalString("Optional room identifier filter."),
  employee_name: optionalString("Optional employee name filter."),
  repo_root: optionalString("Optional repo root filter."),
  branch: optionalString("Optional git branch filter."),
  client: optionalString("Optional client filter, for example codex or claude."),
  since: optionalTimestamp(
    "Optional inclusive start timestamp in ISO-8601 format.",
  ),
  until: optionalTimestamp(
    "Optional inclusive end timestamp in ISO-8601 format.",
  ),
} as const;

const queryEventsSchema = z.object({
  ...eventQueryFields,
  limit: boundedLimit(
    DEFAULT_QUERY_LIMIT,
    MAX_QUERY_LIMIT,
    "Optional number of events to return.",
  ),
});

const searchEventsSchema = z.object({
  ...eventQueryFields,
  limit: boundedLimit(
    DEFAULT_SEARCH_LIMIT,
    MAX_SEARCH_LIMIT,
    "Optional number of results to return.",
  ),
  query: requiredString("The semantic search query."),
});

type EventQueryInput = z.infer<typeof queryEventsSchema>;
type SearchEventsInput = z.infer<typeof searchEventsSchema>;

export interface McpToolOptions {
  auth: SupermanagerAuth;
  config: Pick<ApiConfig, "publicApiUrl" | "publicAppUrl">;
  db: Db;
}

export function createMcpServer(options: McpToolOptions, headers: Headers) {
  const server = new McpServer(SERVER_INFO, {
    instructions: SERVER_INSTRUCTIONS,
  });

  server.registerTool(
    "list_rooms",
    {
      title: "List Rooms",
      description: "List rooms for the current or specified organization.",
      inputSchema: listRoomsSchema,
      annotations: READ_ONLY_TOOL,
    },
    async ({ organization_slug }) => {
      const membership = await loadOrganizationMembership(
        options,
        headers,
        organization_slug,
      );

      return jsonToolResult({
        organization_slug: membership.organization_slug,
        rooms: await options.db.listRoomsForOrganization(
          membership.organization_id,
        ),
      });
    },
  );

  server.registerTool(
    "get_organization_summary",
    {
      title: "Get Organization Summary",
      description: "Read the current organization-level summary snapshot.",
      inputSchema: getOrganizationSummarySchema,
      annotations: READ_ONLY_TOOL,
    },
    async ({ organization_slug }) => {
      const membership = await loadOrganizationMembership(
        options,
        headers,
        organization_slug,
      );

      return jsonToolResult({
        organization_slug: membership.organization_slug,
        status: await options.db.getOrganizationSummaryStatus(
          membership.organization_id,
        ),
        summary: await options.db.getOrganizationSummary(
          membership.organization_id,
        ),
      });
    },
  );

  server.registerTool(
    "get_room_summary",
    {
      title: "Get Room Summary",
      description: "Read the current room-level view composed from the room TLDR, detailed summary, and matching employee TLDRs.",
      inputSchema: getRoomSummarySchema,
      annotations: READ_ONLY_TOOL,
    },
    async ({ room_id }) => {
      const room = await loadAccessibleRoom(options, headers, room_id);

      return jsonToolResult({
        room: roomMetadata(room),
        summary: await options.db.getRoomSummary(room.room_id),
      });
    },
  );

  server.registerTool(
    "get_room_feed",
    {
      title: "Get Room Feed",
      description: "Read raw hook events for a room, newest first.",
      inputSchema: getRoomFeedSchema,
      annotations: READ_ONLY_TOOL,
    },
    async ({ room_id, before_seq, limit }) => {
      const room = await loadAccessibleRoom(options, headers, room_id);

      return jsonToolResult({
        room: roomMetadata(room),
        events: await options.db.getHookEvents(
          room.room_id,
          before_seq,
          undefined,
          limit ?? DEFAULT_FEED_LIMIT,
        ),
      });
    },
  );

  server.registerTool(
    "query_events",
    {
      title: "Query Events",
      description:
        "Query raw events across rooms in an organization using deterministic filters.",
      inputSchema: queryEventsSchema,
      annotations: READ_ONLY_TOOL,
    },
    async (input) => {
      const membership = await loadOrganizationMembership(
        options,
        headers,
        input.organization_slug,
      );
      const filters = buildEventFilters(
        membership.organization_id,
        input,
        input.limit ?? DEFAULT_QUERY_LIMIT,
      );

      return jsonToolResult({
        organization_slug: membership.organization_slug,
        filters: publicFilters(filters),
        events: await queryOrganizationEvents(options.db, filters),
      });
    },
  );

  server.registerTool(
    "search_events",
    {
      title: "Search Events",
      description: "Semantic search over indexed raw hook events in an organization.",
      inputSchema: searchEventsSchema,
      annotations: READ_ONLY_TOOL,
    },
    async (input) => {
      const membership = await loadOrganizationMembership(
        options,
        headers,
        input.organization_slug,
      );
      const filters = {
        ...buildEventFilters(
          membership.organization_id,
          input,
          input.limit ?? DEFAULT_SEARCH_LIMIT,
        ),
        query: input.query,
      };

      return jsonToolResult({
        organization_slug: membership.organization_slug,
        query: input.query,
        filters: publicFilters(filters),
        results: (await searchEvents(options.db, filters)).map(publicSearchResult),
      });
    },
  );

  return server;
}

async function loadOrganizationMembership(
  options: McpToolOptions,
  headers: Headers,
  organizationSlug?: string,
) {
  const viewer = await requireViewer(options.auth, headers);

  return resolveOrganizationMembership(
    options.db,
    viewer.user.id,
    organizationSlug,
    viewer.session.activeOrganizationId ?? null,
  );
}

async function loadAccessibleRoom(
  options: McpToolOptions,
  headers: Headers,
  roomId: string,
) {
  const viewer = await requireViewer(options.auth, headers);
  return requireRoomAccess(options.db, viewer.user.id, roomId);
}

function buildEventFilters(
  organizationId: string,
  input: EventQueryInput | SearchEventsInput,
  limit: number,
) {
  return {
    organizationId,
    roomId: input.room_id,
    employeeName: input.employee_name,
    repoRoot: input.repo_root,
    branch: input.branch,
    client: input.client,
    since: input.since,
    until: input.until,
    limit,
  };
}

function roomMetadata(room: {
  room_id: string;
  name: string;
  organization_slug: string;
  created_at: string;
}) {
  return {
    room_id: room.room_id,
    name: room.name,
    organization_slug: room.organization_slug,
    created_at: room.created_at,
  };
}

function publicFilters(filters: {
  roomId?: string;
  employeeName?: string;
  repoRoot?: string;
  branch?: string;
  client?: string;
  since?: string;
  until?: string;
  limit: number;
}) {
  return {
    room_id: filters.roomId ?? null,
    employee_name: filters.employeeName ?? null,
    repo_root: filters.repoRoot ?? null,
    branch: filters.branch ?? null,
    client: filters.client ?? null,
    since: filters.since ?? null,
    until: filters.until ?? null,
    limit: filters.limit,
  };
}

function publicSearchResult(result: SearchEventResult) {
  return {
    score: result.score,
    search_text: result.search_text,
    event: result.event,
  };
}

function jsonToolResult(data: Record<string, unknown>) {
  return {
    content: [
      {
        type: "text" as const,
        text: JSON.stringify(data, null, 2),
      },
    ],
    structuredContent: data,
  };
}

function requiredString(description: string) {
  return z.string().trim().min(1).describe(description);
}

function optionalString(description: string) {
  return requiredString(description).optional();
}

function optionalTimestamp(description: string) {
  return z
    .string()
    .trim()
    .refine((value) => !Number.isNaN(Date.parse(value)), {
      message: "Must be an ISO-8601 timestamp.",
    })
    .describe(description)
    .optional();
}

function boundedLimit(defaultValue: number, maxValue: number, description: string) {
  return z
    .number()
    .int()
    .min(1)
    .max(maxValue)
    .describe(`${description} Defaults to ${defaultValue}. Max ${maxValue}.`)
    .optional();
}
