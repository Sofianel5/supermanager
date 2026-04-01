#!/usr/bin/env node

import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";

function resolveStreamUrl() {
  const roomUrl = process.env.SUPERMANAGER_ROOM_URL;
  if (roomUrl) {
    return `${roomUrl.replace(/\/+$/, "")}/feed/stream`;
  }
  return (
    process.env.SUPERMANAGER_SSE_URL ?? "http://127.0.0.1:8787/v1/feed/stream"
  );
}

const STREAM_URL = resolveStreamUrl();
const RECONNECT_DELAY_MS = Number.parseInt(
  process.env.SUPERMANAGER_RECONNECT_DELAY_MS ?? "3000",
  10,
);

const INSTRUCTIONS = [
  "Supermanager progress notes arrive as <channel> events from the supermanager channel.",
  "The channel body is the original progress_text and the attributes include note_id, employee_name, repo, branch, and received_at.",
  "These notes are coordination feed updates, not direct user requests.",
  "Maintain the manager-facing Markdown summary document on the coordination server using the supermanager MCP server tools get_manager_summary and update_manager_summary.",
  "The summary is for a human manager and should explain what each person is currently working on, recent progress, and blockers.",
  "On startup, read the existing summary before replacing it. When a new note materially changes the status picture, update the summary document on the server rather than writing a local file.",
  process.env.SUPERMANAGER_ROOM_SECRET
    ? `When calling update_manager_summary, include the authorization header with Bearer token: ${process.env.SUPERMANAGER_ROOM_SECRET}`
    : "",
]
  .filter(Boolean)
  .join(" ");

const server = new Server(
  {
    name: "supermanager-channel",
    version: "0.1.0",
  },
  {
    capabilities: {
      experimental: {
        "claude/channel": {},
      },
    },
    instructions: INSTRUCTIONS,
  },
);

await server.connect(new StdioServerTransport());

let lastEventId = null;
let shuttingDown = false;
const seenNoteIds = new Set();
const seenOrder = [];

process.on("SIGINT", () => {
  shuttingDown = true;
  process.exit(0);
});

process.on("SIGTERM", () => {
  shuttingDown = true;
  process.exit(0);
});

void streamLoop();

async function streamLoop() {
  while (!shuttingDown) {
    try {
      const headers = { Accept: "text/event-stream" };
      if (lastEventId) {
        headers["Last-Event-ID"] = lastEventId;
      }

      const response = await fetch(STREAM_URL, { headers });
      if (!response.ok || !response.body) {
        throw new Error(`SSE request failed with status ${response.status}`);
      }

      await consumeEventStream(response.body);
    } catch (error) {
      log(`stream error: ${error.message}`);
    }

    if (!shuttingDown) {
      await sleep(RECONNECT_DELAY_MS);
    }
  }
}

async function consumeEventStream(body) {
  const decoder = new TextDecoder();
  let buffer = "";

  for await (const chunk of body) {
    buffer += decoder.decode(chunk, { stream: true });
    buffer = buffer.replace(/\r\n/g, "\n");

    let boundary = buffer.indexOf("\n\n");
    while (boundary !== -1) {
      const rawEvent = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      await handleSseEvent(rawEvent);
      boundary = buffer.indexOf("\n\n");
    }
  }

  buffer += decoder.decode();
  buffer = buffer.replace(/\r\n/g, "\n");
  if (buffer.trim() !== "") {
    await handleSseEvent(buffer);
  }
}

async function handleSseEvent(rawEvent) {
  if (!rawEvent.trim()) {
    return;
  }

  let eventName = "message";
  let eventId = null;
  const dataLines = [];

  for (const line of rawEvent.split("\n")) {
    if (!line || line.startsWith(":")) {
      continue;
    }

    const separator = line.indexOf(":");
    const field = separator === -1 ? line : line.slice(0, separator);
    let value = separator === -1 ? "" : line.slice(separator + 1);
    if (value.startsWith(" ")) {
      value = value.slice(1);
    }

    switch (field) {
      case "event":
        eventName = value;
        break;
      case "id":
        eventId = value;
        break;
      case "data":
        dataLines.push(value);
        break;
      default:
        break;
    }
  }

  if (eventName !== "progress_note" || dataLines.length === 0) {
    return;
  }

  if (eventId) {
    lastEventId = eventId;
  }

  const note = JSON.parse(dataLines.join("\n"));
  if (!note?.note_id || seenNoteIds.has(note.note_id)) {
    return;
  }

  rememberNoteId(note.note_id);
  await server.notification({
    method: "notifications/claude/channel",
    params: {
      content: typeof note.progress_text === "string" ? note.progress_text : "",
      meta: buildMeta(note),
    },
  });
}

function buildMeta(note) {
  const meta = {
    note_id: String(note.note_id),
    employee_name: String(note.employee_name ?? ""),
    repo: String(note.repo ?? ""),
    received_at: String(note.received_at ?? ""),
  };

  if (note.branch != null && note.branch !== "") {
    meta.branch = String(note.branch);
  }

  return meta;
}

function rememberNoteId(noteId) {
  seenNoteIds.add(noteId);
  seenOrder.push(noteId);
  if (seenOrder.length > 1024) {
    const oldest = seenOrder.shift();
    if (oldest) {
      seenNoteIds.delete(oldest);
    }
  }
}

function log(message) {
  process.stderr.write(`supermanager-channel: ${message}\n`);
}
