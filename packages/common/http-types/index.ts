// Hand-written HTTP response/request types shared between server and web.
// Types with a Rust counterpart are generated into this directory or ./api-protocol.
import type {
  OrganizationSnapshot,
  ProjectSnapshot,
  SummaryStatus,
} from "../summary-protocol";
export type { ActivityUpdate } from "./ActivityUpdate";
export type { ActivityUpdatesResponse } from "./ActivityUpdatesResponse";

export interface ProjectSummaryResponse {
  last_processed_seq: number;
  status: SummaryStatus;
  summary: ProjectSnapshot;
}

export interface OrganizationSummaryResponse {
  status: SummaryStatus;
  updated_at: string | null;
  summary: OrganizationSnapshot;
}

export interface OrganizationWorkflowDocument {
  path: string;
  content: string;
  updated_at: string;
}

export interface OrganizationWorkflowDocumentsResponse {
  path_root: string;
  documents: OrganizationWorkflowDocument[];
}

export interface OrganizationAgentContextExportFile {
  path: string;
  content: string;
  updated_at: string | null;
}

export interface OrganizationAgentContextExportResponse {
  files: OrganizationAgentContextExportFile[];
}

export interface ProjectListEntry {
  project_id: string;
  name: string;
  created_at: string;
  organization_slug: string;
  bluf_markdown: string;
  member_count: number;
}

export interface ProjectListResponse {
  organization_slug: string;
  projects: ProjectListEntry[];
}

export interface ViewerUser {
  email: string;
  id: string;
  image: string | null;
  name: string;
}

export interface OrganizationMembership {
  organization_id: string;
  organization_name: string;
  organization_slug: string;
  member_count: number;
  role: string;
}

export interface ViewerResponse {
  active_organization_id: string | null;
  has_cli_auth: boolean;
  organizations: OrganizationMembership[];
  user: ViewerUser;
}
