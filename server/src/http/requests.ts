import type { CreateRoomRequest, HookTurnReport } from "../types.js";
import { BadRequestError } from "./errors.js";

export interface RoomPathMatch {
  roomId: string;
  suffix: string;
}

export function parseCreateRoomRequest(input: unknown): CreateRoomRequest {
  const record = expectRecord(input);
  const name = record.name;
  if (typeof name !== "string" || !name.trim()) {
    throw new BadRequestError("name must be a non-empty string");
  }
  return {
    name: name.trim(),
  };
}

export function parseHookTurnReport(input: unknown): HookTurnReport {
  const record = expectRecord(input);
  const employeeName = record.employee_name;
  const client = record.client;
  const repoRoot = record.repo_root;

  if (typeof employeeName !== "string" || !employeeName.trim()) {
    throw new BadRequestError("employee_name must be a non-empty string");
  }
  if (typeof client !== "string" || !client.trim()) {
    throw new BadRequestError("client must be a non-empty string");
  }
  if (typeof repoRoot !== "string" || !repoRoot.trim()) {
    throw new BadRequestError("repo_root must be a non-empty string");
  }
  if (record.branch != null && typeof record.branch !== "string") {
    throw new BadRequestError("branch must be a string or null");
  }
  if (!("payload" in record)) {
    throw new BadRequestError("payload is required");
  }

  return {
    employee_name: employeeName,
    client,
    repo_root: repoRoot,
    branch: record.branch == null ? null : record.branch,
    payload: record.payload,
  };
}

export async function readJson(request: Request): Promise<unknown> {
  try {
    return await request.json();
  } catch {
    throw new BadRequestError("request body must be valid JSON");
  }
}

export function parseLastEventId(rawHeader: string | null): number | null {
  if (rawHeader == null || rawHeader === "") {
    return null;
  }
  return parseOptionalInteger(rawHeader, "Last-Event-ID") ?? null;
}

export function parseOptionalInteger(
  raw: string | undefined,
  label: string,
): number | undefined {
  if (raw == null || raw === "") {
    return undefined;
  }

  const parsed = Number.parseInt(raw, 10);
  if (!Number.isInteger(parsed)) {
    throw new BadRequestError(`${label} must be an integer`);
  }
  return parsed;
}

export function clampInteger(
  raw: string | undefined,
  defaultValue: number,
  min: number,
  max: number,
): number {
  const value = parseOptionalInteger(raw, "limit") ?? defaultValue;
  return Math.min(Math.max(value, min), max);
}

export function matchRoomPath(pathname: string): RoomPathMatch | null {
  const match = /^\/r\/([^/]+)(?:\/(.*))?$/.exec(pathname);
  if (!match) {
    return null;
  }
  return {
    roomId: decodeURIComponent(match[1] ?? ""),
    suffix: match[2] ? `/${match[2]}` : "",
  };
}

function expectRecord(input: unknown): Record<string, unknown> {
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    throw new BadRequestError("request body must be a JSON object");
  }
  return input as Record<string, unknown>;
}
