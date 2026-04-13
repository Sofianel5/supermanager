export function readAuthError(error: { message?: string; status: number; statusText: string }) {
  return error.message || error.statusText || `Request failed with ${error.status}`;
}

export function readMessage(error: unknown) {
  return error instanceof Error ? error.message : "Request failed.";
}
