import type {
  OrganizationSnapshot,
  RoomSnapshot,
} from "@supermanager/common/summary-protocol";

export type {
  CreateRoomRequest,
  CreateRoomResponse,
  FeedResponse,
  HookTurnReport,
  IngestResponse,
  RoomMetadataResponse,
  StoredHookEvent,
} from "@supermanager/common/api-protocol";
export type {
  OrganizationMembership,
  OrganizationSummaryResponse,
  RoomListEntry,
  RoomListResponse,
  RoomSummaryResponse,
  ViewerResponse,
  ViewerUser,
} from "@supermanager/common/http-types";
export type {
  EmployeeSnapshot,
  OrganizationSnapshot,
  RoomBlufSnapshot,
  RoomSnapshot,
  SummaryStatus,
} from "@supermanager/common/summary-protocol";

export interface ConnectionResponse {
  api_key: string;
  api_key_id: string;
  dashboard_url: string;
  room_id: string;
}

export function emptyRoomSnapshot(): RoomSnapshot {
  return {
    bluf_markdown: "",
    detailed_summary_markdown: "",
    employees: [],
  };
}

export function emptyOrganizationSnapshot(): OrganizationSnapshot {
  return {
    bluf_markdown: "",
    rooms: [],
    employees: [],
  };
}

export function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
