import type { StoredHookEvent } from "./types";

type EventName = "hook_event";

export class FeedStreamClient {
  readonly response: Response;
  private readonly encoder = new TextEncoder();
  private readonly keepAliveTimer: Timer;
  private controller: ReadableStreamDefaultController<Uint8Array> | null = null;
  private closed = false;

  constructor(onClose: () => void, origin: string | null) {
    const stream = new ReadableStream<Uint8Array>({
      start: (controller) => {
        this.controller = controller;
      },
      cancel: () => {
        this.closed = true;
        clearInterval(this.keepAliveTimer);
        onClose();
      },
    });

    const headers = new Headers({
      "cache-control": "no-cache, no-transform",
      connection: "keep-alive",
      "content-type": "text/event-stream; charset=utf-8",
      "x-accel-buffering": "no",
    });
    if (origin) {
      headers.set("access-control-allow-credentials", "true");
      headers.set("access-control-allow-origin", origin);
      headers.set("vary", "Origin");
    }

    this.response = new Response(stream, { headers });
    this.enqueue(": stream-open\n\n");
    this.keepAliveTimer = setInterval(() => {
      this.enqueue(": keep-alive\n\n");
    }, 15_000);
  }

  sendHookEvent(event: StoredHookEvent): void {
    this.send("hook_event", JSON.stringify(event), String(event.seq));
  }

  close(): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    clearInterval(this.keepAliveTimer);
    try {
      this.controller?.close();
    } catch {
      // Ignore controller close errors after disconnect.
    }
  }

  private send(event: EventName, data: string, id?: string): void {
    let payload = "";
    if (id) {
      payload += `id: ${id}\n`;
    }
    payload += `event: ${event}\n`;
    for (const line of data.split("\n")) {
      payload += `data: ${line}\n`;
    }
    payload += "\n";
    this.enqueue(payload);
  }

  private enqueue(payload: string): void {
    if (this.closed || !this.controller) {
      return;
    }
    try {
      this.controller.enqueue(this.encoder.encode(payload));
    } catch {
      this.close();
    }
  }
}

export class FeedStreamHub {
  private readonly projects = new Map<string, Set<FeedStreamClient>>();
  private readonly allowedOrigins: Set<string>;

  constructor(allowedOrigins: string[] = []) {
    this.allowedOrigins = new Set(allowedOrigins);
  }

  register(projectId: string, origin: string | null): FeedStreamClient {
    const validatedOrigin = origin && this.allowedOrigins.has(origin) ? origin : null;
    let client: FeedStreamClient;
    const listeners = this.projects.get(projectId) ?? new Set<FeedStreamClient>();

    const cleanup = () => {
      listeners.delete(client);
      if (!listeners.size) {
        this.projects.delete(projectId);
      }
    };

    client = new FeedStreamClient(cleanup, validatedOrigin);
    listeners.add(client);
    this.projects.set(projectId, listeners);

    return client;
  }

  publishHookEvent(projectId: string, event: StoredHookEvent): void {
    const listeners = this.projects.get(projectId);
    if (!listeners) {
      return;
    }
    for (const listener of listeners) {
      listener.sendHookEvent(event);
    }
  }
}
