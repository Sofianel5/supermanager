const STORAGE_KEY = "supermanager.room-secrets";

export type CreatedRoomState = {
  roomId: string;
  secret: string;
};

export function buildCreatedRoomState(
  roomId: string,
  secret: string,
): CreatedRoomState {
  return { roomId, secret };
}

export function stashRoomSecret(roomId: string, secret: string) {
  if (!roomId || !secret || typeof window === "undefined") {
    return;
  }

  const secrets = readStoredSecrets();
  secrets[roomId] = secret;
  window.sessionStorage.setItem(STORAGE_KEY, JSON.stringify(secrets));
}

export function clearRoomSecret(roomId: string) {
  if (!roomId || typeof window === "undefined") {
    return;
  }

  const secrets = readStoredSecrets();
  delete secrets[roomId];
  window.sessionStorage.setItem(STORAGE_KEY, JSON.stringify(secrets));
}

export function buildRoomHash(secret: string) {
  return `#secret=${encodeURIComponent(secret)}`;
}

export function resolveRoomSecret(roomId: string, hash: string, state?: unknown) {
  const stateSecret = readStateSecret(roomId, state);
  if (stateSecret) {
    return stateSecret;
  }

  const hashSecret = readHashSecret(hash);
  if (hashSecret) {
    return hashSecret;
  }

  return readStoredSecrets()[roomId] ?? null;
}

function readStateSecret(roomId: string, state: unknown) {
  if (!state || typeof state !== "object") {
    return null;
  }

  const candidate = state as Partial<CreatedRoomState>;
  if (candidate.roomId !== roomId || typeof candidate.secret !== "string") {
    return null;
  }

  const secret = candidate.secret.trim();
  return secret || null;
}

function readHashSecret(hash: string) {
  if (!hash.startsWith("#")) {
    return null;
  }

  const params = new URLSearchParams(hash.slice(1));
  const secret = params.get("secret")?.trim();
  return secret || null;
}

function readStoredSecrets() {
  if (typeof window === "undefined") {
    return {} as Record<string, string>;
  }

  try {
    const rawValue = window.sessionStorage.getItem(STORAGE_KEY);
    if (!rawValue) {
      return {} as Record<string, string>;
    }

    const parsed = JSON.parse(rawValue);
    if (!parsed || typeof parsed !== "object") {
      return {} as Record<string, string>;
    }

    return Object.fromEntries(
      Object.entries(parsed).filter(
        ([roomId, secret]) => roomId && typeof secret === "string" && secret.length > 0,
      ),
    ) as Record<string, string>;
  } catch {
    return {} as Record<string, string>;
  }
}
