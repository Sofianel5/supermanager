export interface IngestResponse {
  event_id: string;
  received_at: string;
}

export interface FeedResponse {
  events: StoredHookEvent[];
}

export interface Room {
  room_id: string;
  name: string;
  created_at: string;
}

export interface CreateRoomRequest {
  name: string;
}

export interface CreateRoomResponse {
  room_id: string;
  dashboard_url: string;
  join_command: string;
}

export interface RoomMetadataResponse {
  room_id: string;
  name: string;
  created_at: string;
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
