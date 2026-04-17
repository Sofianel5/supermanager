import { normalizeRoomId, type Db } from "../db";
import type {
  EmployeeSnapshot,
  OrganizationSnapshot,
  RoomSnapshot,
} from "../types";
import type { SummaryToolName } from "./protocol";

export interface ToolExecutionResult {
  success: boolean;
  message: string;
}

export async function applySummaryToolCall(
  db: Db,
  scope:
    | { kind: "organization"; organizationId: string }
    | { kind: "room"; roomId: string },
  tool: SummaryToolName,
  argumentsValue: unknown,
): Promise<ToolExecutionResult> {
  if (scope.kind === "room") {
    return applyRoomSummaryToolCall(db, scope.roomId, tool, argumentsValue);
  }

  return applyOrganizationSummaryToolCall(
    db,
    scope.organizationId,
    tool,
    argumentsValue,
  );
}

async function applyRoomSummaryToolCall(
  db: Db,
  roomId: string,
  tool: SummaryToolName,
  argumentsValue: unknown,
): Promise<ToolExecutionResult> {
  switch (tool) {
    case "get_snapshot": {
      const snapshot = await db.getRoomSummary(roomId);
      return {
        success: true,
        message: JSON.stringify(snapshot, null, 2),
      };
    }
    case "set_bluf": {
      const markdown = readRequiredString(argumentsValue, "markdown").trim();
      return mutateRoomSummary(db, roomId, (snapshot) => {
        snapshot.bluf_markdown = markdown;
        return {
          changed: true,
          message: `updated room BLUF for ${normalizeRoomId(roomId)}`,
        };
      });
    }
    case "set_detailed_summary": {
      const markdown = readRequiredString(argumentsValue, "markdown").trim();
      return mutateRoomSummary(db, roomId, (snapshot) => {
        snapshot.detailed_summary_markdown = markdown;
        return {
          changed: true,
          message: `updated room detailed summary for ${normalizeRoomId(roomId)}`,
        };
      });
    }
    case "set_employee_bluf": {
      const employeeName = readRequiredString(
        argumentsValue,
        "employee_name",
      ).trim();
      if (!employeeName) {
        return {
          success: false,
          message: "employee_name must not be empty",
        };
      }

      const markdown = readRequiredString(argumentsValue, "markdown").trim();
      const updatedAt = new Date().toISOString();

      return mutateRoomSummary(db, roomId, (snapshot) => {
        upsertEmployeeBluf(
          snapshot.employees,
          employeeName,
          [normalizeRoomId(roomId)],
          markdown,
          updatedAt,
        );
        return {
          changed: true,
          message: `updated employee BLUF for ${employeeName} in ${normalizeRoomId(roomId)}`,
        };
      });
    }
    case "remove_employee_bluf": {
      const employeeName = readRequiredString(
        argumentsValue,
        "employee_name",
      ).trim();
      return mutateRoomSummary(db, roomId, (snapshot) => {
        const result = removeEmployeeBluf(snapshot, employeeName);
        return {
          changed: result.changed,
          message: result.message,
        };
      });
    }
    default:
      return {
        success: false,
        message: `tool ${tool} is not available for room summaries`,
      };
  }
}

async function applyOrganizationSummaryToolCall(
  db: Db,
  organizationId: string,
  tool: SummaryToolName,
  argumentsValue: unknown,
): Promise<ToolExecutionResult> {
  switch (tool) {
    case "get_snapshot": {
      const snapshot = await db.getOrganizationSummary(organizationId);
      return {
        success: true,
        message: JSON.stringify(snapshot, null, 2),
      };
    }
    case "set_org_bluf": {
      const markdown = readRequiredString(argumentsValue, "markdown").trim();
      return mutateOrganizationSummary(db, organizationId, (snapshot) => {
        snapshot.bluf_markdown = markdown;
        return {
          changed: true,
          message: "updated organization BLUF",
        };
      });
    }
    case "set_employee_bluf": {
      const employeeName = readRequiredString(
        argumentsValue,
        "employee_name",
      ).trim();
      if (!employeeName) {
        return {
          success: false,
          message: "employee_name must not be empty",
        };
      }

      const markdown = readRequiredString(argumentsValue, "markdown").trim();
      const requestedRoomIds = Array.from(
        new Set(
          readRequiredStringArray(argumentsValue, "room_ids").map(
            normalizeRoomId,
          ),
        ),
      );
      const knownRoomIds = new Set(
        await db.listRoomIdsForOrganization(organizationId),
      );
      const roomIds = requestedRoomIds.filter((roomId) =>
        knownRoomIds.has(roomId),
      );
      if (roomIds.length === 0) {
        return {
          success: false,
          message: "room_ids must include at least one valid room for the organization",
        };
      }
      const updatedAt = new Date().toISOString();

      return mutateOrganizationSummary(db, organizationId, (snapshot) => {
        upsertEmployeeBluf(
          snapshot.employees,
          employeeName,
          roomIds,
          markdown,
          updatedAt,
        );
        return {
          changed: true,
          message: `updated employee BLUF for ${employeeName}`,
        };
      });
    }
    case "remove_employee_bluf": {
      const employeeName = readRequiredString(
        argumentsValue,
        "employee_name",
      ).trim();

      return mutateOrganizationSummary(db, organizationId, (snapshot) => {
        const result = removeEmployeeBluf(snapshot, employeeName);
        return {
          changed: result.changed,
          message: result.message,
        };
      });
    }
    default: {
      return {
        success: false,
        message: `tool ${tool} is not available for organization summaries`,
      };
    }
  }
}

async function mutateRoomSummary(
  db: Db,
  roomId: string,
  mutate: (snapshot: RoomSnapshot) => { changed: boolean; message: string },
): Promise<ToolExecutionResult> {
  const snapshot = await db.getRoomSummary(roomId);
  const { changed, message } = mutate(snapshot);
  if (changed) {
    await db.setRoomSummary(roomId, snapshot);
  }

  return {
    success: true,
    message,
  };
}

async function mutateOrganizationSummary(
  db: Db,
  organizationId: string,
  mutate: (snapshot: OrganizationSnapshot) => {
    changed: boolean;
    message: string;
  },
): Promise<ToolExecutionResult> {
  const snapshot = await db.getOrganizationSummary(organizationId);
  const { changed, message } = mutate(snapshot);
  if (changed) {
    await db.setOrganizationSummary(organizationId, snapshot);
  }

  return {
    success: true,
    message,
  };
}

function upsertEmployeeBluf(
  employees: EmployeeSnapshot[],
  employeeName: string,
  roomIds: string[],
  markdown: string,
  updatedAt: string,
): void {
  const employeeKey = normalizeEmployeeName(employeeName);
  const existing = employees.find(
    (employee) => normalizeEmployeeName(employee.employee_name) === employeeKey,
  );

  if (existing) {
    existing.employee_name = employeeName;
    existing.room_ids = roomIds;
    existing.bluf_markdown = markdown;
    existing.last_update_at = updatedAt;
    return;
  }

  employees.push({
    employee_name: employeeName,
    room_ids: roomIds,
    bluf_markdown: markdown,
    last_update_at: updatedAt,
  });
}

function removeEmployeeBluf(
  snapshot: RoomSnapshot | OrganizationSnapshot,
  employeeName: string,
): { changed: boolean; message: string } {
  const employeeKey = normalizeEmployeeName(employeeName);
  const before = snapshot.employees.length;
  snapshot.employees = snapshot.employees.filter(
    (employee) => normalizeEmployeeName(employee.employee_name) !== employeeKey,
  );

  const removed = snapshot.employees.length !== before;
  return {
    changed: removed,
    message: removed
      ? `removed employee BLUF for ${employeeName}`
      : `employee BLUF already absent for ${employeeName}`,
  };
}

function normalizeEmployeeName(value: string): string {
  return value
    .split(/\s+/)
    .filter(Boolean)
    .map((part) => part.toLowerCase())
    .join(" ");
}

function readInputObject(input: unknown, key: string): Record<string, unknown> {
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    throw new Error(`invalid ${key} arguments`);
  }

  return input as Record<string, unknown>;
}

function readRequiredString(input: unknown, key: string): string {
  const value = readInputObject(input, key)[key];
  if (typeof value !== "string") {
    throw new Error(`${key} must be a string`);
  }
  return value;
}

function readRequiredStringArray(input: unknown, key: string): string[] {
  const value = readInputObject(input, key)[key];
  if (!Array.isArray(value)) {
    throw new Error(`${key} must be an array of strings`);
  }

  return value
    .map((entry) => {
      if (typeof entry !== "string") {
        throw new Error(`${key} must be an array of strings`);
      }
      return entry.trim();
    })
    .filter(Boolean);
}
