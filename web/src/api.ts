import type {
  FeedResponse,
  RoomMetadataResponse,
  StoredHookEvent,
} from "./generated";
import type {
  EmployeeSnapshot,
  OrganizationSnapshot,
  RoomBlufSnapshot,
  RoomSnapshot,
  SummaryStatus,
} from "../../server/src/generated/summary-protocol";

export type {
  FeedResponse,
  RoomMetadataResponse,
  StoredHookEvent,
} from "./generated";
export type {
  EmployeeSnapshot,
  OrganizationSnapshot,
  RoomBlufSnapshot,
  RoomSnapshot,
  SummaryStatus,
} from "../../server/src/generated/summary-protocol";

export interface ViewerUser {
  email: string;
  id: string;
  image: string | null;
  name: string;
}

export interface ViewerOrganization {
  organization_id: string;
  organization_name: string;
  organization_slug: string;
  member_count: number;
  role: string;
}

export interface ViewerResponse {
  active_organization_id: string | null;
  has_cli_auth: boolean;
  organizations: ViewerOrganization[];
  user: ViewerUser;
}

export interface RoomListEntry {
  created_at: string;
  name: string;
  organization_slug: string;
  room_id: string;
  bluf_markdown: string;
  employee_count: number;
}

export interface RoomListResponse {
  organization_slug: string;
  rooms: RoomListEntry[];
}

export interface OrganizationSummaryResponse {
  status: SummaryStatus;
  summary: OrganizationSnapshot;
}

export interface CreateRoomResponse {
  dashboard_url: string;
  join_command: string;
  organization_slug: string;
  room_id: string;
}

const API_BASE_URL = normalizeBaseUrl(
  import.meta.env.VITE_API_BASE_URL || "http://127.0.0.1:8787",
);

function normalizeBaseUrl(url: string) {
  return url.replace(/\/+$/, "");
}

function apiUrl(path: string) {
  return `${API_BASE_URL}${path}`;
}

async function readError(response: Response) {
  const body = await response.text();
  return body || `Request failed with ${response.status}`;
}

async function requestJson<T>(path: string, init: RequestInit = {}) {
  const response = await fetch(apiUrl(path), {
    credentials: "include",
    ...init,
  });
  if (!response.ok) {
    throw new Error(await readError(response));
  }
  return (await response.json()) as T;
}

export function getApiBaseUrl() {
  return API_BASE_URL;
}

export const api = {
  createRoom(input: { name: string; organizationSlug?: string | null }) {
    return requestJson<CreateRoomResponse>("/v1/rooms", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        name: input.name,
        organization_slug: input.organizationSlug ?? undefined,
      }),
    });
  },
  getFeed(roomId: string, opts: { limit?: number; before?: number } = {}) {
    const params = new URLSearchParams();
    if (opts.limit != null) params.set("limit", String(opts.limit));
    if (opts.before != null) params.set("before", String(opts.before));
    const qs = params.toString();
    const suffix = qs ? `?${qs}` : "";
    return requestJson<FeedResponse>(
      `/v1/rooms/${encodeURIComponent(roomId)}/feed${suffix}`,
    );
  },
  getMe() {
    return requestJson<ViewerResponse>("/v1/me");
  },
  getRoom(roomId: string) {
    return requestJson<RoomMetadataResponse>(
      `/v1/rooms/${encodeURIComponent(roomId)}`,
    );
  },
  getSummary(roomId: string) {
    return requestJson<RoomSnapshot>(
      `/v1/rooms/${encodeURIComponent(roomId)}/summary`,
    );
  },
  getOrganizationSummary(organizationSlug: string) {
    return requestJson<OrganizationSummaryResponse>(
      `/v1/organizations/${encodeURIComponent(organizationSlug)}/summary`,
    );
  },
  listRooms(organizationSlug?: string) {
    const params = new URLSearchParams();
    if (organizationSlug) {
      params.set("organization_slug", organizationSlug);
    }
    const qs = params.toString();
    return requestJson<RoomListResponse>(`/v1/rooms${qs ? `?${qs}` : ""}`);
  },
  openRoomStream(roomId: string) {
    return new EventSource(
      apiUrl(`/v1/rooms/${encodeURIComponent(roomId)}/feed/stream`),
      { withCredentials: true },
    );
  },
};
