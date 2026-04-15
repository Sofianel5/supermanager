import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js";
import { Elysia } from "elysia";

import { trimUrl } from "../config";
import { httpError } from "../middleware";
import { createMcpServer, type McpToolOptions } from "./tools";

const MCP_ENDPOINT = "/mcp";

export function createMcpRoutes(options: McpToolOptions) {
  const allowedOrigins = new Set([
    trimUrl(options.config.publicApiUrl),
    trimUrl(options.config.publicAppUrl),
  ]);

  return new Elysia()
    .post(MCP_ENDPOINT, async ({ request }) => {
      validateOrigin(request.headers.get("origin"), allowedOrigins);

      const transport = new WebStandardStreamableHTTPServerTransport({
        enableJsonResponse: true,
      });
      const server = createMcpServer(options, request.headers);

      try {
        await server.connect(transport);
        return await transport.handleRequest(request);
      } finally {
        await server.close().catch(() => undefined);
      }
    })
    .get(MCP_ENDPOINT, () => methodNotAllowed())
    .delete(MCP_ENDPOINT, () => methodNotAllowed());
}

function validateOrigin(origin: string | null, allowedOrigins: Set<string>) {
  if (!origin) {
    return;
  }

  if (!allowedOrigins.has(trimUrl(origin))) {
    throw httpError(403, `origin not allowed: ${origin}`);
  }
}

function methodNotAllowed() {
  return new Response("Method Not Allowed", {
    status: 405,
    headers: {
      Allow: "POST",
    },
  });
}
