import type { Db } from "../db.js";
import type { SummaryToolName } from "./protocol.js";
import type { RoomSnapshot } from "../types.js";

export interface ToolExecutionResult {
  success: boolean;
  message: string;
}

export async function applySummaryToolCall(
  db: Db,
  roomId: string,
  tool: SummaryToolName,
  argumentsValue: unknown,
): Promise<ToolExecutionResult> {
  switch (tool) {
    case "get_snapshot": {
      const snapshot = await db.getSummary(roomId);
      return {
        success: true,
        message: JSON.stringify(snapshot, null, 2),
      };
    }
    case "set_bluf": {
      const markdown = readRequiredString(argumentsValue, "markdown").trim();
      return mutateSummary(db, roomId, (snapshot) => {
        snapshot.bluf_markdown = markdown;
        return {
          changed: true,
          message: "updated BLUF",
        };
      });
    }
    case "set_overview": {
      const markdown = readRequiredString(argumentsValue, "markdown").trim();
      return mutateSummary(db, roomId, (snapshot) => {
        snapshot.overview_markdown = markdown;
        return {
          changed: true,
          message: "updated overview",
        };
      });
    }
    case "set_employee_card": {
      const employeeName = readRequiredString(argumentsValue, "employee_name").trim();
      if (!employeeName) {
        return {
          success: false,
          message: "employee_name must not be empty",
        };
      }

      const markdown = readRequiredString(argumentsValue, "markdown").trim();
      const employeeKey = normalizeEmployeeName(employeeName);
      const updatedAt = new Date().toISOString();

      return mutateSummary(db, roomId, (snapshot) => {
        const existing = snapshot.employees.find(
          (employee) => normalizeEmployeeName(employee.employee_name) === employeeKey,
        );

        if (existing) {
          existing.employee_name = employeeName;
          existing.content_markdown = markdown;
          existing.last_update_at = updatedAt;
        } else {
          snapshot.employees.push({
            employee_name: employeeName,
            content_markdown: markdown,
            last_update_at: updatedAt,
          });
        }

        return {
          changed: true,
          message: `updated employee card for ${employeeName}`,
        };
      });
    }
    case "remove_employee_card": {
      const employeeName = readRequiredString(argumentsValue, "employee_name").trim();
      const employeeKey = normalizeEmployeeName(employeeName);

      return mutateSummary(db, roomId, (snapshot) => {
        const before = snapshot.employees.length;
        snapshot.employees = snapshot.employees.filter(
          (employee) => normalizeEmployeeName(employee.employee_name) !== employeeKey,
        );

        const removed = snapshot.employees.length !== before;
        return {
          changed: removed,
          message: removed
            ? `removed employee card for ${employeeName}`
            : `employee card already absent for ${employeeName}`,
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
  roomId: string,
  mutate: (snapshot: RoomSnapshot) => { changed: boolean; message: string },
): Promise<ToolExecutionResult> {
  const snapshot = await db.getSummary(roomId);
  const { changed, message } = mutate(snapshot);
  if (changed) {
    await db.setSummary(roomId, snapshot);
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

function readRequiredString(input: unknown, key: string): string {
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    throw new Error(`invalid ${key} arguments`);
  }

  const value = (input as Record<string, unknown>)[key];
  if (typeof value !== "string") {
    throw new Error(`${key} must be a string`);
  }
  return value;
}
