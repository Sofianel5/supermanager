import type { StoredHookEvent, SummaryStatus } from "../types";

export type SummaryToolName =
  | "get_snapshot"
  | "set_bluf"
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

export interface HostRegenerateOrganizationMessage {
  type: "regenerate_organization";
  organization_id: string;
  events: HostRegenerationEvent[];
  rooms: HostRegenerationRoom[];
  reason: "manual" | "heartbeat";
}

export interface HostRegenerationRoom {
  room_id: string;
  name: string;
}

export interface HostRegenerationEvent extends StoredHookEvent {
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
  | HostRegenerateOrganizationMessage
  | HostToolResultMessage;
