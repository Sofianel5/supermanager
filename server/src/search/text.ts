interface HookEventSearchTextInput {
  employee_name: string;
  client: string;
  repo_root: string;
  branch: string | null;
  payload: unknown;
}

const MAX_TEXT_PART_LENGTH = 1_000;

export function buildHookEventSearchText(input: HookEventSearchTextInput): string {
  const parts = new Set<string>();

  pushPart(parts, input.employee_name);
  pushPart(parts, input.client);
  pushPart(parts, input.repo_root);
  pushPart(parts, input.branch);

  collectPayloadText(parts, input.payload);

  return Array.from(parts).join("\n");
}

function collectPayloadText(parts: Set<string>, value: unknown, key?: string): void {
  if (typeof value === "string") {
    if (shouldSkipKey(key)) {
      return;
    }
    pushPart(parts, value);
    return;
  }

  if (Array.isArray(value)) {
    for (const item of value) {
      collectPayloadText(parts, item, key);
    }
    return;
  }

  if (value && typeof value === "object") {
    for (const [nestedKey, nestedValue] of Object.entries(value)) {
      collectPayloadText(parts, nestedValue, nestedKey);
    }
  }
}

function pushPart(parts: Set<string>, value: string | null | undefined): void {
  const normalized = normalizeText(value);
  if (!normalized) {
    return;
  }
  parts.add(normalized);
}

function normalizeText(value: string | null | undefined): string {
  if (typeof value !== "string") {
    return "";
  }

  const trimmed = value.replace(/\s+/g, " ").trim();
  if (!trimmed) {
    return "";
  }

  return trimmed.length > MAX_TEXT_PART_LENGTH
    ? trimmed.slice(0, MAX_TEXT_PART_LENGTH)
    : trimmed;
}

function shouldSkipKey(key: string | undefined): boolean {
  if (!key) {
    return false;
  }

  const normalized = key.trim().toLowerCase();
  return normalized.endsWith("id");
}
