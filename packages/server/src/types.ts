import type {
  OrganizationSnapshot,
  ProjectSnapshot,
} from "@supermanager/common/summary-protocol";

export type {
  CreateProjectRequest,
  CreateProjectResponse,
  FeedResponse,
  HookTurnReport,
  IngestResponse,
  ProjectMetadataResponse,
  StoredHookEvent,
} from "@supermanager/common/api-protocol";
export type {
  OrganizationMembership,
  OrganizationSummaryResponse,
  ProjectListEntry,
  ProjectListResponse,
  ProjectSummaryResponse,
  ViewerResponse,
  ViewerUser,
} from "@supermanager/common/http-types";
export type {
  MemberSnapshot,
  OrganizationSnapshot,
  ProjectBlufSnapshot,
  ProjectSnapshot,
  SummaryStatus,
} from "@supermanager/common/summary-protocol";

export interface ConnectionResponse {
  api_key: string;
  api_key_id: string;
  dashboard_url: string;
  project_id: string;
}

export function emptyProjectSnapshot(): ProjectSnapshot {
  return {
    bluf_markdown: "",
    detailed_summary_markdown: "",
    members: [],
  };
}

export function emptyOrganizationSnapshot(): OrganizationSnapshot {
  return {
    bluf_markdown: "",
    projects: [],
    members: [],
  };
}

export function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
