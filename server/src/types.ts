export interface IngestResponse {
  event_id: string;
  received_at: string;
}

export interface FeedResponse {
  events: StoredHookEvent[];
}

export interface CreateRoomRequest {
  name: string;
  organization_slug?: string | null;
}

export interface CreateRoomResponse {
  room_id: string;
  dashboard_url: string;
  join_command: string;
  organization_slug: string;
}

export interface RoomMetadataResponse {
  room_id: string;
  name: string;
  created_at: string;
  organization_slug: string;
  join_command: string;
}

export interface RoomListEntry {
  room_id: string;
  name: string;
  created_at: string;
  organization_slug: string;
  bluf_markdown: string;
  employee_count: number;
}

export interface ConnectionResponse {
  api_key: string;
  api_key_id: string;
  dashboard_url: string;
  room_id: string;
}

export interface OrganizationMembership {
  organization_id: string;
  organization_name: string;
  organization_slug: string;
  role: string;
}

export interface ViewerResponse {
  active_organization_id: string | null;
  has_cli_auth: boolean;
  organizations: OrganizationMembership[];
  user: {
    email: string;
    id: string;
    image: string | null;
    name: string;
  };
}

export interface EmployeeSnapshot {
  employee_name: string;
  content_markdown: string;
  last_update_at: string;
}

export interface RoomSnapshot {
  bluf_markdown: string;
  overview_markdown: string;
  employees: EmployeeSnapshot[];
}

export interface HookTurnReport {
  employee_name: string;
  client: string;
  repo_root: string;
  branch: string | null;
  payload: unknown;
}

export interface StoredHookEvent {
  seq: number;
  event_id: string;
  received_at: string;
  employee_name: string;
  client: string;
  repo_root: string;
  branch: string | null;
  payload: unknown;
}

export type SummaryStatus = "generating" | "ready" | "error";

export function emptyRoomSnapshot(): RoomSnapshot {
  return {
    bluf_markdown: "",
    overview_markdown: "",
    employees: [],
  };
}
