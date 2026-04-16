const EMBEDDING_MODEL = "text-embedding-3-large";
const EMBEDDINGS_URL = "https://api.openai.com/v1/embeddings";

interface EmbeddingsResponse {
  data?: Array<{
    embedding?: unknown;
  }>;
}

export async function embedText(text: string): Promise<number[]> {
  const apiKey = Bun.env.CODEX_API_KEY?.trim();
  if (!apiKey) {
    throw new Error("missing CODEX_API_KEY");
  }

  const response = await fetch(EMBEDDINGS_URL, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${apiKey}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      input: text,
      model: EMBEDDING_MODEL,
    }),
  });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(
      `embeddings request failed (${response.status}): ${body || response.statusText}`,
    );
  }

  const payload = (await response.json()) as EmbeddingsResponse;
  const embedding = payload.data?.[0]?.embedding;
  if (!Array.isArray(embedding) || embedding.some((value) => typeof value !== "number")) {
    throw new Error("invalid embeddings response");
  }

  return embedding;
}
