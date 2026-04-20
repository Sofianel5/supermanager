// Hand-written HTTP response/request types shared between server and web.
// Types with a Rust counterpart live in ./api-protocol instead.
import type {
  OrganizationSnapshot,
  RoomSnapshot,
  SummaryStatus,
} from "../summary-protocol";

export interface RoomSummaryResponse {
  last_processed_seq: number;
  status: SummaryStatus;
  summary: RoomSnapshot;
}

export interface OrganizationSummaryResponse {
  status: SummaryStatus;
  updated_at: string | null;
  summary: OrganizationSnapshot;
}

export interface RoomListEntry {
  room_id: string;
  name: string;
  created_at: string;
  organization_slug: string;
  bluf_markdown: string;
  employee_count: number;
}

export interface RoomListResponse {
  organization_slug: string;
  rooms: RoomListEntry[];
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
