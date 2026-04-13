export function corsHeaders(): Record<string, string> {
  return {
    "access-control-allow-origin": "*",
    "access-control-allow-methods": "GET, POST, OPTIONS",
    "access-control-allow-headers": "Content-Type, Last-Event-ID",
  };
}

export function jsonResponse(payload: unknown, status = 200): Response {
  return new Response(JSON.stringify(payload), {
    status,
    headers: {
      ...corsHeaders(),
      "content-type": "application/json; charset=utf-8",
    },
  });
}

export function textResponse(message: string, status: number): Response {
  return new Response(message, {
    status,
    headers: {
      ...corsHeaders(),
      "content-type": "text/plain; charset=utf-8",
    },
  });
}

export function noContentResponse(): Response {
  return new Response(null, {
    status: 204,
    headers: corsHeaders(),
  });
}
