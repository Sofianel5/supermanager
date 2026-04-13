import type { StoredHookEvent, SummaryStatus } from "./types.js";

type EventName = "hook_event" | "summary_status";

export class FeedStreamClient {
  readonly response: Response;
  private readonly encoder = new TextEncoder();
  private readonly keepAliveTimer: Timer;
  private controller: ReadableStreamDefaultController<Uint8Array> | null = null;
  private closed = false;

  constructor(onClose: () => void) {
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

    this.response = new Response(stream, {
      headers: {
        "access-control-allow-origin": "*",
        "cache-control": "no-cache, no-transform",
        connection: "keep-alive",
        "content-type": "text/event-stream; charset=utf-8",
        "x-accel-buffering": "no",
      },
    });
    this.keepAliveTimer = setInterval(() => {
      this.enqueue(": keep-alive\n\n");
    }, 15_000);
  }

  sendHookEvent(event: StoredHookEvent): void {
    this.send("hook_event", JSON.stringify(event), String(event.seq));
  }

  sendSummaryStatus(status: SummaryStatus): void {
    this.send("summary_status", JSON.stringify({ status }));
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
  private readonly rooms = new Map<string, Set<FeedStreamClient>>();

  register(roomId: string): FeedStreamClient {
    let client: FeedStreamClient;
    const listeners = this.rooms.get(roomId) ?? new Set<FeedStreamClient>();

    const cleanup = () => {
      listeners.delete(client);
      if (!listeners.size) {
        this.rooms.delete(roomId);
      }
    };

    client = new FeedStreamClient(cleanup);
    listeners.add(client);
    this.rooms.set(roomId, listeners);

    return client;
  }

  publishHookEvent(roomId: string, event: StoredHookEvent): void {
    const listeners = this.rooms.get(roomId);
    if (!listeners) {
      return;
    }
    for (const listener of listeners) {
      listener.sendHookEvent(event);
    }
  }

  publishSummaryStatus(roomId: string, status: SummaryStatus): void {
    const listeners = this.rooms.get(roomId);
    if (!listeners) {
      return;
    }
    for (const listener of listeners) {
      listener.sendSummaryStatus(status);
    }
  }
}
