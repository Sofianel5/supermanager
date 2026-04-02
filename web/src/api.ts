export type PublicConfigResponse = {
  install_command: string;
};

export type CreateRoomResponse = {
  install_command: string;
  room_id: string;
  secret: string;
  dashboard_url: string;
  join_command: string;
};

export type RoomMetadataResponse = {
  room_id: string;
  name: string;
  created_at: string;
};

export type StoredHookEvent = {
  event_id: string;
  received_at: string;
  employee_name: string;
  client: string;
  repo_root: string;
  branch?: string | null;
  payload: unknown;
};

export type FeedResponse = {
  events: StoredHookEvent[];
};

const API_BASE_URL = normalizeBaseUrl(
  import.meta.env.VITE_API_BASE_URL || "http://127.0.0.1:8787",
);

function normalizeBaseUrl(url: string) {
  return url.replace(/\/+$/, "");
}

function withSecret(path: string, secret?: string | null) {
  if (!secret) {
    return path;
  }

  const separator = path.includes("?") ? "&" : "?";
  return `${path}${separator}secret=${encodeURIComponent(secret)}`;
}

function apiUrl(path: string, secret?: string | null) {
  return `${API_BASE_URL}${withSecret(path, secret)}`;
}

async function readError(response: Response) {
  const body = await response.text();
  return body || `Request failed with ${response.status}`;
}

async function requestJson<T>(
  path: string,
  init?: RequestInit,
  secret?: string | null,
) {
  const response = await fetch(apiUrl(path, secret), init);
  if (!response.ok) {
    throw new Error(await readError(response));
  }
  return (await response.json()) as T;
}

async function requestText(path: string, secret?: string | null) {
  const response = await fetch(apiUrl(path, secret));
  if (!response.ok) {
    throw new Error(await readError(response));
  }
  return response.text();
}

export function getApiBaseUrl() {
  return API_BASE_URL;
}

export const api = {
  getPublicConfig() {
    return requestJson<PublicConfigResponse>("/config");
  },
  createRoom(name: string) {
    return requestJson<CreateRoomResponse>("/v1/rooms", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ name }),
    });
  },
  getRoom(roomId: string, secret: string) {
    return requestJson<RoomMetadataResponse>(
      `/r/${encodeURIComponent(roomId)}`,
      undefined,
      secret,
    );
  },
  getFeed(roomId: string, secret: string) {
    return requestJson<FeedResponse>(
      `/r/${encodeURIComponent(roomId)}/feed`,
      undefined,
      secret,
    );
  },
  getSummary(roomId: string, secret: string) {
    return requestText(`/r/${encodeURIComponent(roomId)}/summary`, secret);
  },
  openRoomStream(roomId: string, secret: string) {
    return new EventSource(
      apiUrl(`/r/${encodeURIComponent(roomId)}/feed/stream`, secret),
    );
  },
};
