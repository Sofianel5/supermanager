import type { StoredHookEvent, SummaryStatus } from "../types";

export type SummaryToolName =
  | "get_snapshot"
  | "set_bluf"
  | "set_overview"
  | "set_employee_card"
  | "remove_employee_card";

export interface AgentToolCallMessage {
  type: "tool_call";
  id: string;
  room_id: string;
  tool: SummaryToolName;
  arguments: unknown;
}

export interface AgentSummaryStatusMessage {
  type: "summary_status";
  room_id: string;
  status: SummaryStatus;
}

export type AgentMessage = AgentToolCallMessage | AgentSummaryStatusMessage;

export interface HostEnqueueEventMessage {
  type: "enqueue_event";
  room_id: string;
  event: StoredHookEvent;
}

export interface HostToolResultMessage {
  type: "tool_result";
  id: string;
  success: boolean;
  message: string;
}

export type HostMessage = HostEnqueueEventMessage | HostToolResultMessage;
