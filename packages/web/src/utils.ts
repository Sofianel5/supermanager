import { useState } from "react";

export function readAuthError(error: { message?: string; status: number; statusText: string }) {
  return error.message || error.statusText || `Request failed with ${error.status}`;
}

export function readMessage(error: unknown) {
  return error instanceof Error ? error.message : "Request failed.";
}

export function useCopyHandler() {
  const [copiedValue, setCopiedValue] = useState<string | null>(null);

  async function copy(label: string, value: string) {
    await navigator.clipboard.writeText(value);
    setCopiedValue(label);
    window.setTimeout(() => {
      setCopiedValue((current) => (current === label ? null : current));
    }, 1800);
  }

  return { copiedValue, copy } as const;
}
