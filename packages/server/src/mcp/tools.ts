import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import * as z from "zod/v4";

import type { SupermanagerAuth } from "../auth";
import type { ApiConfig } from "../config";
import type { Db } from "../db";
import {
  requireProjectAccess,
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
  "Use get_organization_summary for the org-level snapshot, get_project_summary for a single project view, query_events or search_events for raw historical evidence, and get_updates for recent timestamped statements scoped to org / project / member.";

const DEFAULT_FEED_LIMIT = 25;
const DEFAULT_QUERY_LIMIT = 25;
const DEFAULT_SEARCH_LIMIT = 10;
const DEFAULT_UPDATES_LIMIT = 25;
const MAX_FEED_LIMIT = 100;
const MAX_QUERY_LIMIT = 100;
const MAX_SEARCH_LIMIT = 25;
const MAX_UPDATES_LIMIT = 100;

const READ_ONLY_TOOL = {
  readOnlyHint: true,
  destructiveHint: false,
  idempotentHint: true,
  openWorldHint: false,
} as const;

const listProjectsSchema = z.object({
  organization_slug: optionalString(
    "Optional organization slug. Defaults to the active organization.",
  ),
});

const getProjectSummarySchema = z.object({
  project_id: requiredString("The project identifier."),
});

const getOrganizationSummarySchema = z.object({
  organization_slug: optionalString(
    "Optional organization slug. Defaults to the active organization.",
  ),
});

const getProjectFeedSchema = z.object({
  project_id: requiredString("The project identifier."),
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

const getUpdatesSchema = z.object({
  organization_slug: optionalString(
    "Optional organization slug. Defaults to the active organization.",
  ),
  scope: z
    .enum(["organization", "project", "member"])
    .describe("Optional scope filter.")
    .optional(),
  project_id: optionalString(
    "Optional project filter. Required when scope='project'.",
  ),
  member_user_id: optionalString(
    "Optional member user id filter. Required when scope='member'.",
  ),
  before_seq: z
    .number()
    .int()
    .min(1)
    .describe("Optional sequence cursor for pagination.")
    .optional(),
  limit: boundedLimit(
    DEFAULT_UPDATES_LIMIT,
    MAX_UPDATES_LIMIT,
    "Optional number of updates to return.",
  ),
});

const eventQueryFields = {
  organization_slug: optionalString(
    "Optional organization slug. Defaults to the active organization.",
  ),
  project_id: optionalString("Optional project identifier filter."),
  member_name: optionalString("Optional member name filter."),
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
    "list_projects",
    {
      title: "List Projects",
      description: "List projects for the current or specified organization.",
      inputSchema: listProjectsSchema,
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
        projects: await options.db.listProjectsForOrganization(
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

      const summary = await options.db.getOrganizationSummaryResponse(
        membership.organization_id,
      );

      return jsonToolResult({
        organization_slug: membership.organization_slug,
        ...summary,
      });
    },
  );

  server.registerTool(
    "get_project_summary",
    {
      title: "Get Project Summary",
      description: "Read the current project-level view composed from the project TLDR, detailed summary, and matching member TLDRs.",
      inputSchema: getProjectSummarySchema,
      annotations: READ_ONLY_TOOL,
    },
    async ({ project_id }) => {
      const project = await loadAccessibleProject(options, headers, project_id);

      return jsonToolResult({
        project: projectMetadata(project),
        summary: await options.db.getProjectSummary(project.project_id),
      });
    },
  );

  server.registerTool(
    "get_project_feed",
    {
      title: "Get Project Feed",
      description: "Read raw hook events for a project, newest first.",
      inputSchema: getProjectFeedSchema,
      annotations: READ_ONLY_TOOL,
    },
    async ({ project_id, before_seq, limit }) => {
      const project = await loadAccessibleProject(options, headers, project_id);

      return jsonToolResult({
        project: projectMetadata(project),
        events: await options.db.getHookEvents(
          project.project_id,
          before_seq,
          undefined,
          limit ?? DEFAULT_FEED_LIMIT,
        ),
      });
    },
  );

  server.registerTool(
    "get_updates",
    {
      title: "Get Updates",
      description:
        "Read recent timestamped updates emitted by the workflow agents, scoped to organization, project, or member.",
      inputSchema: getUpdatesSchema,
      annotations: READ_ONLY_TOOL,
    },
    async ({
      organization_slug,
      scope,
      project_id,
      member_user_id,
      before_seq,
      limit,
    }) => {
      const membership = await loadOrganizationMembership(
        options,
        headers,
        organization_slug,
      );

      if (project_id != null) {
        await loadAccessibleProject(options, headers, project_id);
      }

      const updates = await options.db.getUpdates(membership.organization_id, {
        scope,
        projectId: project_id,
        memberUserId: member_user_id,
        beforeSeq: before_seq,
        limit: limit ?? DEFAULT_UPDATES_LIMIT,
      });

      return jsonToolResult({
        organization_slug: membership.organization_slug,
        scope: scope ?? null,
        updates,
      });
    },
  );

  server.registerTool(
    "query_events",
    {
      title: "Query Events",
      description:
        "Query raw events across projects in an organization using deterministic filters.",
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

async function loadAccessibleProject(
  options: McpToolOptions,
  headers: Headers,
  projectId: string,
) {
  const viewer = await requireViewer(options.auth, headers);
  return requireProjectAccess(options.db, viewer.user.id, projectId);
}

function buildEventFilters(
  organizationId: string,
  input: EventQueryInput | SearchEventsInput,
  limit: number,
) {
  return {
    organizationId,
    projectId: input.project_id,
    memberName: input.member_name,
    repoRoot: input.repo_root,
    branch: input.branch,
    client: input.client,
    since: input.since,
    until: input.until,
    limit,
  };
}

function projectMetadata(project: {
  project_id: string;
  name: string;
  organization_slug: string;
  created_at: string;
}) {
  return {
    project_id: project.project_id,
    name: project.name,
    organization_slug: project.organization_slug,
    created_at: project.created_at,
  };
}

function publicFilters(filters: {
  projectId?: string;
  memberName?: string;
  repoRoot?: string;
  branch?: string;
  client?: string;
  since?: string;
  until?: string;
  limit: number;
}) {
  return {
    project_id: filters.projectId ?? null,
    member_name: filters.memberName ?? null,
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
