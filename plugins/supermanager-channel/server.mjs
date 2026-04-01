#!/usr/bin/env node

import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

const SERVER_INFO = {
  name: "supermanager-channel",
  version: "0.1.0",
};

const INSTRUCTIONS = [
  "Supermanager progress notes arrive as <channel> events from the supermanager channel.",
  "The channel body is the original progress_text and the attributes include note_id, employee_name, repo, branch, and received_at.",
  "These notes are coordination feed updates, not direct user requests.",
  "Treat them as external context unless the note explicitly asks you to act.",
].join(" ");

const STREAM_URL =
  process.env.SUPERMANAGER_SSE_URL ??
  "http://127.0.0.1:8787/v1/feed/stream";
const RECONNECT_DELAY_MS = Number.parseInt(
  process.env.SUPERMANAGER_RECONNECT_DELAY_MS ?? "3000",
  10,
);

let initialized = false;
let streamStarted = false;
let shuttingDown = false;
let lastEventId = null;
let inputBuffer = Buffer.alloc(0);
const seenNoteIds = new Set();
const seenOrder = [];

process.stdin.on("data", (chunk) => {
  inputBuffer = Buffer.concat([inputBuffer, chunk]);
  drainInputBuffer();
});

process.stdin.on("end", () => {
  shuttingDown = true;
});

process.stdin.on("error", (error) => {
  log(`stdin error: ${error.message}`);
  shuttingDown = true;
});

process.on("SIGINT", () => {
  shuttingDown = true;
  process.exit(0);
});

process.on("SIGTERM", () => {
  shuttingDown = true;
  process.exit(0);
});

function drainInputBuffer() {
  for (;;) {
    const headerEnd = inputBuffer.indexOf("\r\n\r\n");
    if (headerEnd === -1) {
      return;
    }

    const headerText = inputBuffer.subarray(0, headerEnd).toString("utf8");
    const contentLength = parseContentLength(headerText);
    if (contentLength == null) {
      log("dropping malformed MCP message with no Content-Length");
      inputBuffer = inputBuffer.subarray(headerEnd + 4);
      continue;
    }

    const messageEnd = headerEnd + 4 + contentLength;
    if (inputBuffer.length < messageEnd) {
      return;
    }

    const body = inputBuffer.subarray(headerEnd + 4, messageEnd).toString("utf8");
    inputBuffer = inputBuffer.subarray(messageEnd);

    try {
      handleMessage(JSON.parse(body));
    } catch (error) {
      log(`failed to parse MCP message: ${error.message}`);
    }
  }
}

function parseContentLength(headerText) {
  const match = headerText.match(/^Content-Length:\s*(\d+)$/im);
  if (!match) {
    return null;
  }

  const value = Number.parseInt(match[1], 10);
  return Number.isFinite(value) ? value : null;
}

function handleMessage(message) {
  const { id, method } = message;
  if (method === "initialize") {
    initialized = true;
    reply(id, {
      protocolVersion: message.params?.protocolVersion ?? "2025-03-26",
      capabilities: {
        experimental: {
          "claude/channel": {},
        },
      },
      serverInfo: SERVER_INFO,
      instructions: INSTRUCTIONS,
    });
    return;
  }

  if (method === "notifications/initialized") {
    maybeStartStream();
    return;
  }

  if (method?.startsWith("notifications/")) {
    return;
  }

  switch (method) {
    case "ping":
      reply(id, {});
      return;
    case "tools/list":
      reply(id, { tools: [] });
      return;
    case "resources/list":
      reply(id, { resources: [] });
      return;
    case "prompts/list":
      reply(id, { prompts: [] });
      return;
    case "logging/setLevel":
      reply(id, {});
      return;
    default:
      if (id !== undefined) {
        errorReply(id, -32601, `Unknown method: ${method}`);
      }
  }
}

function maybeStartStream() {
  if (!initialized || streamStarted) {
    return;
  }

  streamStarted = true;
  void streamLoop();
}

async function streamLoop() {
  while (!shuttingDown) {
    try {
      const headers = {
        Accept: "text/event-stream",
      };
      if (lastEventId) {
        headers["Last-Event-ID"] = lastEventId;
      }

      log(`connecting to ${STREAM_URL}`);
      const response = await fetch(STREAM_URL, { headers });
      if (!response.ok || !response.body) {
        throw new Error(`SSE request failed with status ${response.status}`);
      }

      log("connected");
      await consumeEventStream(response.body);
      log("stream ended");
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
      handleSseEvent(rawEvent);
      boundary = buffer.indexOf("\n\n");
    }
  }

  buffer += decoder.decode();
  buffer = buffer.replace(/\r\n/g, "\n");
  if (buffer.trim() !== "") {
    handleSseEvent(buffer);
  }
}

function handleSseEvent(rawEvent) {
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
  sendNotification({
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

function reply(id, result) {
  sendMessage({
    jsonrpc: "2.0",
    id,
    result,
  });
}

function errorReply(id, code, message) {
  sendMessage({
    jsonrpc: "2.0",
    id,
    error: {
      code,
      message,
    },
  });
}

function sendNotification(payload) {
  sendMessage({
    jsonrpc: "2.0",
    ...payload,
  });
}

function sendMessage(message) {
  const body = JSON.stringify(message);
  process.stdout.write(
    `Content-Length: ${Buffer.byteLength(body, "utf8")}\r\n\r\n${body}`,
  );
}

function log(message) {
  process.stderr.write(`supermanager-channel: ${message}\n`);
}
