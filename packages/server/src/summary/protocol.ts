import type { StoredHookEvent, SummaryStatus } from "../types";

export type SummaryToolName =
  | "get_snapshot"
  | "set_bluf"
  | "set_detailed_summary"
  | "set_org_bluf"
  | "set_employee_bluf"
  | "remove_employee_bluf";

export interface AgentToolCallMessage {
  type: "tool_call";
  id: string;
  scope: "organization" | "room";
  target_id: string;
  tool: SummaryToolName;
  arguments: unknown;
}

export interface AgentSummaryStatusMessage {
  type: "summary_status";
  scope: "organization" | "room";
  target_id: string;
  status: SummaryStatus;
}

export type AgentMessage = AgentToolCallMessage | AgentSummaryStatusMessage;

export interface HostEnqueueRoomEventMessage {
  type: "enqueue_room_event";
  room_id: string;
  room_name: string;
  event: StoredHookEvent;
}

export interface HostOrganizationHeartbeatMessage {
  type: "organization_heartbeat";
  organization_id: string;
  events: HostOrganizationHeartbeatEvent[];
  rooms: HostOrganizationHeartbeatRoom[];
}

export interface HostOrganizationHeartbeatRoom {
  room_id: string;
  name: string;
}

export interface HostOrganizationHeartbeatEvent extends StoredHookEvent {
  room_id: string;
  room_name: string;
}

export interface HostToolResultMessage {
  type: "tool_result";
  id: string;
  success: boolean;
  message: string;
}

export type HostMessage =
  | HostEnqueueRoomEventMessage
  | HostOrganizationHeartbeatMessage
  | HostToolResultMessage;
