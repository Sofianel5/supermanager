import { normalizeRoomId, type Db } from "../db";
import type { OrganizationSnapshot } from "../types";
import type { SummaryToolName } from "./protocol";

export interface ToolExecutionResult {
  success: boolean;
  message: string;
}

export async function applySummaryToolCall(
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
      return mutateSummary(db, organizationId, (snapshot) => {
        snapshot.bluf_markdown = markdown;
        return {
          changed: true,
          message: "updated organization BLUF",
        };
      });
    }
    case "set_room_bluf": {
      const roomId = normalizeRoomId(
        readRequiredString(argumentsValue, "room_id").trim(),
      );
      if (!roomId) {
        return {
          success: false,
          message: "room_id must not be empty",
        };
      }

      const markdown = readRequiredString(argumentsValue, "markdown").trim();
      const updatedAt = new Date().toISOString();

      return mutateSummary(db, organizationId, (snapshot) => {
        const existing = snapshot.rooms.find((room) => room.room_id === roomId);

        if (existing) {
          existing.bluf_markdown = markdown;
          existing.last_update_at = updatedAt;
        } else {
          snapshot.rooms.push({
            room_id: roomId,
            bluf_markdown: markdown,
            last_update_at: updatedAt,
          });
        }

        return {
          changed: true,
          message: `updated room BLUF for ${roomId}`,
        };
      });
    }
    case "remove_room_bluf": {
      const roomId = normalizeRoomId(
        readRequiredString(argumentsValue, "room_id").trim(),
      );
      return mutateSummary(db, organizationId, (snapshot) => {
        const before = snapshot.rooms.length;
        snapshot.rooms = snapshot.rooms.filter((room) => room.room_id !== roomId);

        const removed = snapshot.rooms.length !== before;
        return {
          changed: removed,
          message: removed ? `removed room BLUF for ${roomId}` : `room BLUF already absent for ${roomId}`,
        };
      });
    }
    case "set_employee_bluf": {
      const employeeName = readRequiredString(argumentsValue, "employee_name").trim();
      if (!employeeName) {
        return {
          success: false,
          message: "employee_name must not be empty",
        };
      }

      const markdown = readRequiredString(argumentsValue, "markdown").trim();
      const roomIds = Array.from(
        new Set(
          readRequiredStringArray(argumentsValue, "room_ids").map(normalizeRoomId),
        ),
      );
      const employeeKey = normalizeEmployeeName(employeeName);
      const updatedAt = new Date().toISOString();

      return mutateSummary(db, organizationId, (snapshot) => {
        const existing = snapshot.employees.find(
          (employee) => normalizeEmployeeName(employee.employee_name) === employeeKey,
        );

        if (existing) {
          existing.employee_name = employeeName;
          existing.room_ids = roomIds;
          existing.bluf_markdown = markdown;
          existing.last_update_at = updatedAt;
        } else {
          snapshot.employees.push({
            employee_name: employeeName,
            room_ids: roomIds,
            bluf_markdown: markdown,
            last_update_at: updatedAt,
          });
        }

        return {
          changed: true,
          message: `updated employee BLUF for ${employeeName}`,
        };
      });
    }
    case "remove_employee_bluf": {
      const employeeName = readRequiredString(argumentsValue, "employee_name").trim();
      const employeeKey = normalizeEmployeeName(employeeName);

      return mutateSummary(db, organizationId, (snapshot) => {
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
      });
    }
    default: {
      const unreachable: never = tool;
      return {
        success: false,
        message: `unknown tool: ${String(unreachable)}`,
      };
    }
  }
}

async function mutateSummary(
  db: Db,
  organizationId: string,
  mutate: (snapshot: OrganizationSnapshot) => { changed: boolean; message: string },
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
