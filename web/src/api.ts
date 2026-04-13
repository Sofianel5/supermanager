import { createParser, type EventSourceMessage } from "eventsource-parser";
import type {
  FeedResponse,
  CurrentUserResponse,
  InviteResponse,
  RoomMetadataResponse,
  RoomSnapshot,
} from "./generated";

export type {
  CurrentUserResponse,
  EmployeeSnapshot,
  FeedResponse,
  InviteResponse,
  RoomMetadataResponse,
  RoomSnapshot,
  StoredHookEvent,
} from "./generated";

const API_BASE_URL = normalizeBaseUrl(
  import.meta.env.VITE_API_BASE_URL || "http://127.0.0.1:8787",
);

export type AccessTokenGetter = () => Promise<string>;

export type RoomStreamHandlers = {
  onEvent?: (event: EventSourceMessage) => void;
  onError?: (error: unknown) => void;
  onOpen?: () => void;
};

export type RoomStreamSubscription = {
  close: () => void;
};

function normalizeBaseUrl(url: string) {
  return url.replace(/\/+$/, "");
}

function apiUrl(path: string) {
  return `${API_BASE_URL}${path}`;
}

export class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

async function readError(response: Response) {
  const body = await response.text();
  return body || `Request failed with ${response.status}`;
}

function authorizedInit(accessToken: string, init: RequestInit = {}) {
  const headers = new Headers(init.headers);
  headers.set("Authorization", `Bearer ${accessToken}`);
  return {
    ...init,
    headers,
  };
}

async function requestJson<T>(path: string, accessToken: string, init?: RequestInit) {
  const response = await fetch(apiUrl(path), authorizedInit(accessToken, init));
  if (!response.ok) {
    throw new ApiError(response.status, await readError(response));
  }
  return (await response.json()) as T;
}

export function getApiBaseUrl() {
  return API_BASE_URL;
}

export const api = {
  getCurrentUser(accessToken: string) {
    return requestJson<CurrentUserResponse>("/v1/me", accessToken);
  },
  createEmailInvite(accessToken: string, roomId: string, targetEmail: string) {
    return requestJson<InviteResponse>(
      `/r/${encodeURIComponent(roomId)}/invites/email`,
      accessToken,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ target_email: targetEmail }),
      },
    );
  },
  getRoom(accessToken: string, roomId: string) {
    return requestJson<RoomMetadataResponse>(
      `/r/${encodeURIComponent(roomId)}`,
      accessToken,
    );
  },
  getFeed(
    accessToken: string,
    roomId: string,
    opts: { limit?: number; before?: number } = {},
  ) {
    const params = new URLSearchParams();
    if (opts.limit != null) params.set("limit", String(opts.limit));
    if (opts.before != null) params.set("before", String(opts.before));
    const qs = params.toString();
    const suffix = qs ? `?${qs}` : "";
    return requestJson<FeedResponse>(
      `/r/${encodeURIComponent(roomId)}/feed${suffix}`,
      accessToken,
    );
  },
  getSummary(accessToken: string, roomId: string) {
    return requestJson<RoomSnapshot>(
      `/r/${encodeURIComponent(roomId)}/summary`,
      accessToken,
    );
  },
  openRoomStream(
    roomId: string,
    getAccessToken: AccessTokenGetter,
    handlers: RoomStreamHandlers,
  ): RoomStreamSubscription {
    let closed = false;
    let controller: AbortController | null = null;
    let lastEventId: string | undefined;

    const connect = async () => {
      while (!closed) {
        controller = new AbortController();
        try {
          const accessToken = await getAccessToken();
          const headers = new Headers({
            Accept: "text/event-stream",
            Authorization: `Bearer ${accessToken}`,
          });
          if (lastEventId) {
            headers.set("Last-Event-ID", lastEventId);
          }

          const response = await fetch(
            apiUrl(`/r/${encodeURIComponent(roomId)}/feed/stream`),
            {
              headers,
              signal: controller.signal,
            },
          );
          if (!response.ok) {
            throw new ApiError(response.status, await readError(response));
          }
          if (!response.body) {
            throw new Error("Room stream did not return a body.");
          }

          handlers.onOpen?.();

          const parser = createParser({
            onEvent(event) {
              if (event.id) {
                lastEventId = event.id;
              }
              handlers.onEvent?.(event);
            },
          });

          const reader = response.body.getReader();
          const decoder = new TextDecoder();
          while (!closed) {
            const { done, value } = await reader.read();
            if (done) {
              break;
            }
            parser.feed(decoder.decode(value, { stream: true }));
          }

          if (closed) {
            return;
          }
          throw new Error("Room stream closed.");
        } catch (error) {
          if (closed || isAbortError(error)) {
            return;
          }

          handlers.onError?.(error);
          if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
            return;
          }

          await delay(1500);
        }
      }
    };

    void connect();

    return {
      close() {
        closed = true;
        controller?.abort();
      },
    };
  },
};

function isAbortError(error: unknown) {
  return error instanceof Error && error.name === "AbortError";
}

function delay(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}
