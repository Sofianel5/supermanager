export class BadRequestError extends Error {}

export class NotFoundError extends Error {}

export function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
