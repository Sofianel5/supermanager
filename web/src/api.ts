import type {
  FeedResponse,
  RoomMetadataResponse,
  RoomSnapshot,
} from "./generated";

export type {
  EmployeeSnapshot,
  FeedResponse,
  RoomMetadataResponse,
  RoomSnapshot,
  StoredHookEvent,
} from "./generated";

const API_BASE_URL = normalizeBaseUrl(
  import.meta.env.VITE_API_BASE_URL || "http://127.0.0.1:8787",
);

function normalizeBaseUrl(url: string) {
  return url.replace(/\/+$/, "");
}

function apiUrl(path: string) {
  return `${API_BASE_URL}${path}`;
}

async function readError(response: Response) {
  const body = await response.text();
  return body || `Request failed with ${response.status}`;
}

async function requestJson<T>(path: string, init?: RequestInit) {
  const response = await fetch(apiUrl(path), init);
  if (!response.ok) {
    throw new Error(await readError(response));
  }
  return (await response.json()) as T;
}

export function getApiBaseUrl() {
  return API_BASE_URL;
}

export const api = {
  getRoom(roomId: string) {
    return requestJson<RoomMetadataResponse>(`/r/${encodeURIComponent(roomId)}`);
  },
  getFeed(roomId: string, opts: { limit?: number; before?: number } = {}) {
    const params = new URLSearchParams();
    if (opts.limit != null) params.set("limit", String(opts.limit));
    if (opts.before != null) params.set("before", String(opts.before));
    const qs = params.toString();
    const suffix = qs ? `?${qs}` : "";
    return requestJson<FeedResponse>(
      `/r/${encodeURIComponent(roomId)}/feed${suffix}`,
    );
  },
  getSummary(roomId: string) {
    return requestJson<RoomSnapshot>(`/r/${encodeURIComponent(roomId)}/summary`);
  },
  openRoomStream(roomId: string) {
    return new EventSource(apiUrl(`/r/${encodeURIComponent(roomId)}/feed/stream`));
  },
};
